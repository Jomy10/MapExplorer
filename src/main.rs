use std::fs;

use cxx::let_cxx_string;
use log::*;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::Appender;
use log4rs::encode::pattern::PatternEncoder;
use log4rs::config::Logger;
use map_explorer::ffi::ostream;
use map_explorer::{app, mapnik_config, new_Pipe, new_PipeInputStream, new_PipeOutputStream, setup_mapnik, UniqueSendPtr};
use regex::Regex;

fn main() -> anyhow::Result<()> {
    // Set up logging
    let stdout_appender = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{h({l})} {d(%H:%M:%S)} [{t}] {m}\n")))
        .build();
    let config = log4rs::Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
        .logger(Logger::builder().build("map_explorer", LevelFilter::Trace))
        .build(log4rs::config::Root::builder()
            .appender("stdout")
            .build(LevelFilter::Info)
        )?;
    _ = log4rs::init_config(config).unwrap();

    // Parse args
    let mut args = std::env::args();
    let progname = args.next().unwrap(); // always present
    let mapfile = args.next().unwrap_or("map.xml".to_string());
    let basepath = args.next().unwrap_or(".".to_string());

    if args.next().is_some() {
        return Err(anyhow::format_err!("Invalid argument.\nUsage: {} [mapnik stylesheet path] [basepath]", progname));
    }

    let projdirs = directories::ProjectDirs::from("be", "jonaseveraert", "MapExplorer").unwrap();
    let cache_dir = projdirs.cache_dir();
    if !cache_dir.exists() {
        fs::create_dir(cache_dir)?;
    }

    let inifile = cache_dir.join("MapExplorer.ini");
    let cachefile = cache_dir.join("cache.json");

    info!("inifile: {}", inifile.display());
    info!("cachefile: {}", cachefile.display());

    // TODO: logging
    let mut pipe = new_Pipe()?;
    let pipeout = new_PipeOutputStream(pipe.clone())?;
    let pipein = new_PipeInputStream(pipe.clone())?;
    let pipein = UniqueSendPtr { ptr: pipein };

    let os: *mut ostream = unsafe { std::mem::transmute(pipeout.as_mut_ptr()) };
    unsafe { map_explorer::set_logging(os); }
    map_explorer::ffi::clog_redirect();

    _ = std::thread::spawn(move || -> anyhow::Result<()> {
        let pipein = pipein;
        let mut pipein = pipein.ptr;
        let_cxx_string!(cxxbuf = "");
        let mapnik_log_regex = Regex::new(r"Mapnik LOG> \d+-\d+-\d+ \d+:\d+:\d+: ")?;
        loop {
            let input = pipein.pin_mut();
            _ = map_explorer::ffi::getline(unsafe { std::mem::transmute(input) }, cxxbuf.as_mut())?;

            if cxxbuf.len() == 0 { continue } // TODO: can close?

            let str = cxxbuf.to_string_lossy();
            if str.as_ref().starts_with("Mapnik LOG>") {
                let str = mapnik_log_regex.replace(str.as_ref(), "");
                if str.contains("error") {
                    error!(target: "Mapnik", "{}", str);
                } else {
                    info!(target: "Mapnik", "{}", str);
                }
            } else {
                info!(target: "CxxMapExplorer", "{}", str);
            }
        }
        // return Ok(());
    });

    setup_mapnik(&mapnik_config::input_plugins_dir()?, &mapnik_config::fonts_dir()?)?;

    let w = 800;
    let h = 600;

    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = app::MapExplorer::new(w, h, mapfile, basepath, inifile, cachefile)?;
    event_loop.run_app(&mut app)?;

    map_explorer::ffi::restore_clog();
    unsafe { map_explorer::ffi::close_pipe(pipe.pin_mut_unchecked())? };

    Ok(())
}
