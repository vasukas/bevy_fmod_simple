use std::path::PathBuf;

fn main() {
    // crate root directory, same one `build.rs` file is in
    let crate_root = std::env::current_dir().unwrap();

    // path to FMOD static & shared libraries
    let fmod_libs_path = crate_root.join("fmod").join("lib").join(
        match std::env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
            "windows" => "x64_windows",
            "linux" => "x64_linux",
            os => panic!("Unknown target OS: {}", os),
        },
    );

    build_fmod_cpp_bridge(&crate_root, &fmod_libs_path);
    copy_fmod_runtime_to_output_dir(&fmod_libs_path);
}

fn build_fmod_cpp_bridge(crate_root: &PathBuf, fmod_libs_path: &PathBuf) {
    // link crate to shared libraries
    println!(
        "cargo:rustc-link-search=native={}",
        fmod_libs_path.to_str().unwrap()
    );
    println!("cargo:rustc-link-lib=dylib=fmod");
    println!("cargo:rustc-link-lib=dylib=fmodL");

    // build C++ library & link it
    let rust_source = "src/bridge.rs";
    let cpp_dir = crate_root.join("src-cpp");
    cxx_build::bridge(rust_source)
        .file(cpp_dir.join("bridge.cpp"))
        .flag_if_supported("-std=c++17") // GCC
        .flag_if_supported("/std:c++17") // MSVC
        .compile("fmod_bridge");

    // rebuild if source files change
    println!("cargo:rerun-if-changed={}", rust_source);
    for file in ["bridge.cpp", "bridge.h"] {
        println!(
            "cargo:rerun-if-changed={}",
            cpp_dir.join(file).to_str().unwrap()
        );
    }
}

fn copy_fmod_runtime_to_output_dir(fmod_libs_path: &PathBuf) {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    for from in list_all_files(fmod_libs_path) {
        let is_static_library = from
            .extension()
            .map(|ext| ext == ".lib")
            .unwrap_or_default();

        if !is_static_library {
            let to = out_dir.join(from.file_name().unwrap());
            std::fs::copy(from, to).unwrap();
        }
    }
}

/// List of all files and symlinks in directory, non-recursive
fn list_all_files(source_path: &PathBuf) -> Vec<PathBuf> {
    std::fs::read_dir(source_path)
        .unwrap()
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            if ty.is_file() || ty.is_symlink() {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect()
}
