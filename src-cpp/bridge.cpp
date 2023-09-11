#include <cstdio>
#include <cstdarg>
#include <cstring>

#include "bridge.h"
#include "../fmod/include/fmod_errors.h"

// I use __has_include so this cpp file can be viewed from C++ editor
// without setting custom include paths. C++ sucks.
#if __has_include("../../../target/cxxbridge/bevy_fmod_simple/src/bridge.rs.h")
	#include "../../../target/cxxbridge/bevy_fmod_simple/src/bridge.rs.h"
	// there is always 2 errors with this still
#else
	#include "bevy_fmod_simple/src/bridge.rs.h"
	// generated Rust bindings (used when building with cargo)
#endif

//
// Utility functions
//

// this __attribute__ construction enables compile-time check of arguments types for printf-like functions.
// can be used only on function declarations, not definitions. C sucks too.
#ifndef __GNUC__
	// GCC extension
	#define __attribute__(a)
#endif

/// Print log message
static void report_msg(bool is_info, const char *fmt, ...) __attribute__ ((format (printf, 2, 3)));
void report_msg(bool is_info, const char *fmt, ...) {
	// do a printf to string
	va_list va;
	va_start(va, fmt);
	const int sn = 4096; // max string length, should be more than enough
	char s[sn] = {};
	int n = vsnprintf(s, sn, fmt, va);
	if (n < 0) {
		n = strlen(fmt) + 1;
		memcpy(s, fmt, n);
	}

	// bridge_log_* functions are defined in Rust
	(is_info ? bridge_log_info : bridge_log_error)(rust::Slice{reinterpret_cast<const uint8_t*>(s), static_cast<size_t>(n)});
	va_end(va);
}

#define info_msg(...) report_msg(true, __VA_ARGS__)
#define error_msg(...) report_msg(false, __VA_ARGS__)

/// Check if result is a error and log it if it is
#define ERRCHECK(_result) ERRCHECK_fn(_result, __LINE__)
static bool ERRCHECK_fn(FMOD_RESULT result, int line) {
    if (result != FMOD_OK) {
        error_msg("FMOD error (bridge.cpp:%d): %d - %s", line, result, FMOD_ErrorString(result));
		return false;
    }
	return true;
}

static FMOD_VECTOR vector(Vector v) {
	return {v.x, v.y, v.z};
}

// insert new item in sparse array (has vacant places with nullptr value) and return index
template<typename T>
int sparse_array_insert(std::vector<T*>& objects, T* new_object) {
	size_t i=0; for (; i<objects.size(); ++i) if (!objects[i]) break; // find null
	if (i == objects.size()) objects.emplace_back(); // no null, increase vector size
	objects[i] = new_object;
	return i;
}

//

bool Bridge::init(InitParams params) {
	//
	// library initialization

	info_msg("FMOD static library version: %d.%d.%d", FMOD_VERSION >> 16, (FMOD_VERSION >> 8) & 0xff, FMOD_VERSION & 0xff);

	result = FMOD::System_Create(&system);
	if (!ERRCHECK(result))
		return false;

	unsigned int fmod_version = 0;
	result = system->getVersion(&fmod_version);
	ERRCHECK(result);
	if (fmod_version != FMOD_VERSION)
		error_msg("FMOD dynamic library version differs! It is %d.%d.%d", fmod_version >> 16, (fmod_version >> 8) & 0xff, fmod_version & 0xff);

	result = system->setSoftwareChannels(params.max_active_channels); // MUST be called before system->init!
	ERRCHECK(result);

	result = system->init(
		params.max_virtual_channels,
		FMOD_INIT_NORMAL |
			FMOD_INIT_CHANNEL_LOWPASS | // required for 3D geometry occlusion?
			FMOD_INIT_VOL0_BECOMES_VIRTUAL | // disables playback for sounds which have near-0 volume
			FMOD_INIT_3D_RIGHTHANDED, // same coordinate system bevy uses
		nullptr
	);
	if (!ERRCHECK(result))
		return false;
	
	//
	// apply settings

	FMOD_ADVANCEDSETTINGS settings = {};
	settings.cbSize = sizeof(FMOD_ADVANCEDSETTINGS);

	result = system->getAdvancedSettings(&settings);
	ERRCHECK(result);

	// linear volume below which channel is considered to be completely silent
	// TODO(later): unhardcode - this can be changed at any time
	settings.vol0virtualvol = 0.01;

	result = system->setAdvancedSettings(&settings);
	ERRCHECK(result);

	return true;
}

Bridge::~Bridge() {
	for (auto& reverb : reverbs) {
		if (reverb)
			reverb->release();
	}

	for (auto& geometry : geometries) {
		if (geometry)
			geometry->release();
	}

	for (auto& channel : channels) {
		if (channel)
			channel->stop();
	}

	for (auto& sound : sounds) {
		if (sound)
			sound->release();
	}

	for (auto& group : groups) {
		group.second->release();
	}

	result = system->close();
	ERRCHECK(result);
	
	result = system->release();
	ERRCHECK(result);
}

FMOD::ChannelGroup* Bridge::get_group(int user_id) {
	auto& group = groups[user_id];
	if (!group) { // create group with default parameters if it doesn't exist
		GroupParams params;
		params.user_id = user_id;
		params.volume = 1.;
		update_group(params);
	}
	return group;
}
	
void Bridge::update() {
	result = system->update();
	ERRCHECK(result);
}

void Bridge::update_engine(EngineParams params) {
	result = system->set3DSettings(params.doppler_scale, params.distance_scale, params.rolloff_scale);
	ERRCHECK(result);

	result = system->setGeometrySettings(params.max_world_size);
	ERRCHECK(result);
}
	
void Bridge::update_listener(ListenerParams params) {
	auto position = vector(params.position);
	auto velocity = vector(params.velocity);
	auto forward = vector(params.forward);
	auto up = vector(params.up);

	result = system->set3DListenerAttributes(0, &position, &velocity, &forward, &up);
	ERRCHECK(result);
}

void Bridge::update_group(GroupParams params) {
	auto& group = groups[params.user_id];

	// create group if needed
	if (!group) {
		const auto group_name = std::to_string(params.user_id);

		result = system->createChannelGroup(group_name.c_str(), &group);
		if (!ERRCHECK(result))
			return;

		// TODO(later): is it possible to reduce ramp duration?
		result = group->setVolumeRamp(true); // enable smooth change of volume
		ERRCHECK(result);
	}

	result = group->setVolume(params.volume);
	ERRCHECK(result);
}

int Bridge::load_audio_file(AudioFileParams params) {
	int flags = FMOD_3D | FMOD_LOOP_NORMAL; // allow spatial usage and being looped
	FMOD::Sound* sound = nullptr;

	if (!params.filename.empty()) {
		flags |= FMOD_CREATESTREAM; // don't load whole file into memory

		result = system->createSound(params.filename.c_str(), flags, nullptr, &sound);
		if (!ERRCHECK(result)) {
			info_msg("Path to the file: \"%s\"", params.filename.c_str());
			return -1;
		}
	}
	else if (!params.file_contents.empty()) {
		flags |= FMOD_OPENMEMORY;

		FMOD_CREATESOUNDEXINFO exinfo = {};
		exinfo.cbsize = sizeof(FMOD_CREATESOUNDEXINFO);
    	exinfo.length = params.file_contents.size();

		result = system->createSound((const char*) params.file_contents.data(), flags, &exinfo, &sound);
		if (!ERRCHECK(result))
			return -1;
	}
	else {
		error_msg("No sound data");
		return -1;
	}
	
	return sparse_array_insert(sounds, sound);
}

void Bridge::free_audio_file(int i) {
	auto& sound = sounds.at(i);

	result = sound->release();
	ERRCHECK(result);

	sound = nullptr;
}

int Bridge::play_channel(ChannelParams params) {
	auto& source = sounds.at(params.file_id);

	FMOD::Channel* channel = nullptr;
	result = system->playSound(source, get_group(params.group_id), true, &channel); // sound starts paused
	if (!ERRCHECK(result))
		return -1;

	// set all parameters (before unpausing the sound)

	if (params.is_positional) {
		result = channel->setMode(FMOD_3D);
		ERRCHECK(result);

		auto position = vector(params.position);
		auto velocity = vector(params.velocity);

		result = channel->set3DAttributes(&position, &velocity);
		ERRCHECK(result);

		result = channel->set3DMinMaxDistance(params.min_distance, params.max_distance);
		ERRCHECK(result);
	}
	else {
		result = channel->setMode(FMOD_2D);
		ERRCHECK(result);
	}

	if (params.startup_delay) {
		// Delay is set used global clock (or clock of parent DSP).
		// We need to get current clock value and convert delay into clock ticks.

		unsigned long long parentclock = 0; // delay uses parent clock, not channel one
		int ticks_per_second = 0; // sample rate = clock ticks per second

		result = channel->getDSPClock(nullptr, &parentclock);
		ERRCHECK(result);

		result = system->getSoftwareFormat(&ticks_per_second, nullptr, nullptr);
		ERRCHECK(result);

		const auto microseconds_per_second = 1000. * 1000.;
		const auto delay = ticks_per_second * (params.startup_delay / microseconds_per_second);
		
		result = channel->setDelay(parentclock + delay, 0);
		ERRCHECK(result);
	}
	else {
		result = channel->setDelay(0, 0); // in case channel got re-used // TODO(later): is this needed?
		ERRCHECK(result);
	}

	result = channel->setLoopCount(params.looped ? -1 : 0); // -1 for infinite repeat
	ERRCHECK(result);

	result = channel->setVolume(params.volume);
	ERRCHECK(result);

	result = channel->setPitch(params.pitch);
	ERRCHECK(result);

	result = channel->setPriority(params.priority);
	ERRCHECK(result);

	// all parameters are set, start playback

	result = channel->setPaused(false);
	ERRCHECK(result);

	return sparse_array_insert(channels, channel);
}

bool Bridge::update_channel(int i, ChannelUpdateParams params) {
	auto& channel = channels.at(i);

	bool is_playing = false;
	result = channel->isPlaying(&is_playing);
	
	if (result == FMOD_ERR_INVALID_HANDLE || result == FMOD_ERR_CHANNEL_STOLEN)
		return false; // sound stopped or stolen (reused, i.e. for higher priority sound)
	if (!ERRCHECK(result))
		return false;
	
	if (params.set_position) {
		auto position = vector(params.position);
		auto velocity = vector(params.velocity);

		result = channel->set3DAttributes(&position, &velocity);
		ERRCHECK(result);
	}

	if (params.set_volume_etc) {
		result = channel->setVolume(params.volume);
		ERRCHECK(result);

		result = channel->setPitch(params.pitch);
		ERRCHECK(result);

		result = channel->setPriority(params.priority);
		ERRCHECK(result);
	}

	return is_playing;
}

bool Bridge::is_playing_channel(int i) {
	auto& channel = channels.at(i);

	bool is_playing = false;
	result = channel->isPlaying(&is_playing);
	
	if (result != FMOD_ERR_INVALID_HANDLE && result != FMOD_ERR_CHANNEL_STOLEN) {
		if (!ERRCHECK(result)) // sound stopped or stolen
			return false;
	}

	return is_playing;
}

void Bridge::free_channel(int i) {
	auto& channel = channels.at(i);

	result = channel->stop();
	
	if (result != FMOD_ERR_INVALID_HANDLE && result != FMOD_ERR_CHANNEL_STOLEN)
		ERRCHECK(result); // sound stopped or stolen

	channel = nullptr;
}

int Bridge::add_geometry(Geometry params) {
	int vertex_count = 0;
	for (auto& polygon : params.polygons)
		vertex_count += polygon.vertices.size();

	// info_msg("Adding geometry: %d polygons, %d vertices", int(params.polygons.size()), vertex_count);

	FMOD::Geometry* geometry = nullptr;
	result = system->createGeometry(params.polygons.size(), vertex_count, &geometry);
	if (!ERRCHECK(result))
		return -1;

	for (auto& polygon : params.polygons) {
		std::vector<FMOD_VECTOR> vertices;
		vertices.reserve(polygon.vertices.size());
		for (auto& vertex : polygon.vertices)
			vertices.push_back(vector(vertex));

		int polygon_index = 0; // unused value
		result = geometry->addPolygon(params.direct_occlusion, params.reverb_occlusion, true, vertices.size(), vertices.data(), &polygon_index);
		ERRCHECK(result);
	}

	return sparse_array_insert(geometries, geometry);
}

void Bridge::free_geometry(int i) {
	auto& geometry = geometries.at(i);

	result = geometry->release();
	ERRCHECK(result);

	geometry = nullptr;
}

int Bridge::add_reverb(Reverb params) {
	FMOD::Reverb3D* reverb = nullptr;
	result = system->createReverb3D(&reverb);
	if (!ERRCHECK(result))
		return -1;
	
	FMOD_REVERB_PROPERTIES prop = FMOD_PRESET_GENERIC;
	prop.DecayTime = params.decay_time;
	prop.EarlyDelay = params.early_delay;
	prop.LateDelay = params.late_delay;
	prop.HFReference = params.hf_reference;
	prop.HFDecayRatio = params.hf_decay_ratio;
	prop.Diffusion = params.diffusion;
	prop.Density = params.density;
	prop.LowShelfFrequency = params.low_shelf_frequency;
	prop.LowShelfGain = params.low_shelf_gain;
	prop.HighCut = params.high_cut;
	prop.EarlyLateMix = params.early_late_mix;
	prop.WetLevel = params.wet_level;

	result = reverb->setProperties(&prop);
	ERRCHECK(result);

	FMOD_VECTOR position = vector(params.position);
	result = reverb->set3DAttributes(&position, params.min_dist, params.max_dist);
	ERRCHECK(result);

	return sparse_array_insert(reverbs, reverb);
}

void Bridge::free_reverb(int i) {
	auto& reverb = reverbs.at(i);

	result = reverb->release();
	ERRCHECK(result);

	reverb = nullptr;
}

std::unique_ptr<Bridge> create(InitParams params) {
	auto p = std::make_unique<Bridge>();
	if (!p->init(std::move(params)))
		return {};
	return p;
}
