use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use git2::Repository;

#[allow(unused)]
macro_rules! warn {
    ($($tokens: tt)*) => {
        println!("cargo:warning={}", format!($($tokens)*))
    }
}

fn main() -> anyhow::Result<()> {
    //===========//
    // Link Poco //
    //===========//

    // let target_dir = env::var("OUT_DIR").ok().map(|p| PathBuf::from(p))
    //     .ok_or(env::var("CARGO_TARGET_DIR").ok().map(|p| PathBuf::from(p)))
    //     .unwrap_or(Path::new(env!("CARGO_MANIFEST_DIR"))
    //         .join("target")
    //         .join(env::var("PROFILE").unwrap())
    //     );

    // let poco_dir = target_dir.join("poco");
    let poco_dir = Path::new("target/poco");
    if !poco_dir.exists() || std::fs::read_dir(&poco_dir).iter().next().is_none() {
        // TODO: copy to one directory (target/); doesn't need to be copied for both debug and release
        _ = match Repository::clone("https://github.com/pocoproject/poco", &poco_dir) {
            Ok(repo) => repo,
            Err(e) => panic!("Failed to clone poco: {}", e),
        };
    }

    let poco_out = cmake::Config::new(&poco_dir)
        // .out_dir(poco_dir.join("build"))
        .define("ENABLE_XML", "OFF")
        .define("ENABLE_JSON", "OFF")
        .define("ENABLE_NET", "OFF")
        .define("ENABLE_NETSSL", "OFF")
        .define("ENABLE_CRYPTO", "OFF")
        .define("ENABLE_JWT", "OFF")
        .define("ENABLE_DATA", "OFF")
        .define("ENABLE_DATA_SQLITE", "OFF")
        .define("ENABLE_DATA_MYSQL", "OFF")
        .define("ENABLE_DATA_POSTGRESQL", "OFF")
        .define("ENABLE_DATA_ODBC", "OFF")
        .define("ENABLE_MONGODB", "OFF")
        .define("ENABLE_REDIS", "OFF")
        .define("ENABLE_PDF", "OFF")
        .define("ENABLE_UTIL", "ON")
        .define("ENABLE_ZIP", "OFF")
        .define("ENABLE_SEVENZIP", "OFF")
        .define("ENABLE_APACHECONNECTOR", "OFF")
        .define("ENABLE_CPPPARSER", "OFF")
        .define("ENABLE_ENCODINGS", "OFF")
        .define("ENABLE_ENCODINGS_COMPILER", "OFF")
        .define("ENABLE_PAGECOMPILER", "OFF")
        .define("ENABLE_PAGECOMPILER_FILE2PAGE", "OFF")
        .define("ENABLE_POCODOC", "OFF")
        .define("ENABLE_TESTS", "OFF")
        .define("ENABLE_SAMPLES", "OFF")
        .define("ENABLE_LONG_RUNNING_TESTS", "OFF")
        .define("POCO_UNBUNDLED", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .build();

    let poco_lib_dir = poco_out.join("lib");
    // panic!("{}", poco_lib_dir.display());
    println!("cargo:rustc-link-arg=-L{}", poco_lib_dir.display());
    println!("cargo:rustc-link-lib=PocoFoundationd");

    let poco_include = poco_out.join("include");

    //=============//
    // Link Mapnik //
    //=============//

    let mut b = cxx_build::bridge("src/map_renderer.rs");
    b.cpp(true);
    b.std("c++20");

    b.file("MapRenderer/MapRenderer.cpp");
    b.file("MapRenderer/glue.cpp");
    // b.file("MapRenderer/PipeStream.cpp");
    b.file("MapRenderer/log.cpp");

    // Headers
    let out = Command::new("mapnik-config")
        .args(["--includes", "--dep-includes"])
        .output()?;
    assert!(out.status.success());
    let paths = shlex::split(&String::from_utf8(out.stdout)?.trim()).unwrap();
    assert!(paths.len() > 0);

    //, "/opt/mapnik/include/mapnik/deps"
    let include_paths = paths.iter()
        .map(|include| &(include.trim())[2..])
        .chain(["./MapRenderer/include", poco_include.to_str().unwrap()].into_iter());
    for path in include_paths {
        b.include(path);
    }

    // cflags
    let out = Command::new("mapnik-config")
        .args(["--cflags"])
        .output()?;
    assert!(out.status.success());
    let cflags = shlex::split(&String::from_utf8(out.stdout)?.trim()).unwrap();

    for flag in cflags {
        b.flag(flag);
    }
    b.flag("-DMAPNIK_THREADSAFE");

    // Linker flags
    let out = Command::new("mapnik-config")
        .args(["--libs", "--dep-libs", "--ldflags"])
        .output()?;
    assert!(out.status.success());
    let link_args = shlex::split(&String::from_utf8(out.stdout)?.trim()).unwrap();
    for arg in link_args.iter() {
        if let Some(arg) = arg.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={}", arg);
        } else {
            println!("cargo:rustc-link-arg={}", arg);
        }
    }

    // Build
    b.compile("map-renderer");
    println!("cargo:rerun-if-changed=./MapRenderer/include/MapRenderer.hpp");
    println!("cargo:rerun-if-changed=./MapRenderer/include/glue.hpp");
    println!("cargo:rerun-if-changed=./MapRenderer/include/log.hpp");
    println!("cargo:rerun-if-changed=./MapRenderer/MapRenderer.cpp");
    println!("cargo:rerun-if-changed=./MapRenderer/glue.cpp");
    println!("cargo:rerun-if-changed=./MapRenderer/log.cpp");

    //============//
    // Link Cairo //
    //============//

    pkg_config::Config::new()
        .probe("cairo")?;

    let bindings = bindgen::Builder::default()
        .header("cairo-wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()?;

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not defined"));
    bindings.write_to_file(out_path.join("cairo_bindings.rs"))?;

    Ok(())
}
