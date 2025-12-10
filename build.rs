use std::env;
use std::path::PathBuf;
use std::process::Command;

#[allow(unused)]
macro_rules! warn {
    ($($tokens: tt)*) => {
        println!("cargo:warning={}", format!($($tokens)*))
    }
}

fn main() -> anyhow::Result<()> {
    //=============//
    // Link Mapnik //
    //=============//

    let mut b = cxx_build::bridge("src/map_renderer.rs");
    b.cpp(true);
    b.std("c++20");

    b.file("MapRenderer/MapRenderer.cpp");
    b.file("MapRenderer/glue.cpp");

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
        .chain(["./MapRenderer/include"].into_iter());
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
    b.compile("mapnik-rs");
    println!("cargo:rerun-if-changed=./MapRenderer/include/MapRenderer.hpp");
    println!("cargo:rerun-if-changed=./MapRenderer/include/glue.hpp");
    println!("cargo:rerun-if-changed=./MapRenderer/MapRenderer.cpp");
    println!("cargo:rerun-if-changed=./MapRenderer/glue.cpp");

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
//     // Link Mapnik //

//     let mut b = cxx_build::bridge("src/map/mapnik.rs");
//     b.cpp(true);
//     b.std("c++20");

//     b.file("mapnik-wrapper.cpp");

//     // Headers
//     let out = Command::new("mapnik-config")
//         .args(["--includes", "--dep-includes"])
//         .output()?;
//     assert!(out.status.success());
//     let paths = shlex::split(&String::from_utf8(out.stdout)?.trim()).unwrap();
//     assert!(paths.len() > 0);

//     let include_paths = paths.iter().map(|include| &(include.trim())[2..]).chain([".", "/opt/mapnik/include/mapnik/deps"].into_iter());
//     for path in include_paths {
//         b.include(path);
//     }

//     // Cflags
//     let out = Command::new("mapnik-config")
//         .args(["--cflags"])
//         .output()?;
//     assert!(out.status.success());
//     let cflags = shlex::split(&String::from_utf8(out.stdout)?.trim()).unwrap();

//     for flag in cflags {
//         b.flag(flag);
//     }

//     // Linker flags
//     let out = Command::new("mapnik-config")
//         .args(["--libs", "--dep-libs", "--ldflags"])
//         .output()?;
//     assert!(out.status.success());
//     let link_args = shlex::split(&String::from_utf8(out.stdout)?.trim()).unwrap();
//     for arg in link_args.iter() {
//         println!("cargo:rustc-link-arg={}", arg);
//     }

//     println!("cargo:rustc-link=mapnik");

//     // Build
//     b.compile("mapnik-rs");
//     println!("cargo:rerun-if-changed=mapnik-wrapper.hpp");

//     // Link Cairo //

//     pkg_config::Config::new()
//         .probe("cairo")?;

//     let bindings = bindgen::Builder::default()
//         .header("cairo-wrapper.h")
//         .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
//         .generate()?;

//     let out_path = PathBuf::from(env::var("OUT_DIR").unwrap_or("target".to_string()));
//     bindings.write_to_file(out_path.join("cairo_bindings.rs"))?;

//     Ok(())
// }
