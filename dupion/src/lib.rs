use std::io::Write;

use anyhow::bail;
use anyhow::ensure;
use anyhow::Result as AnyhowResult;

#[macro_export]
macro_rules! dprint {
    ($($arg:tt)*) => {{
        $crate::dprint_imp(format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! dprintln {
    ($($arg:tt)*) => {{
        $crate::dprintln_imp(format_args!($($arg)*));
    }};
}

pub mod driver;
pub mod state;
pub mod vfs;
pub mod phase;
pub mod opts;
pub mod util;
pub mod group;
pub mod process;
pub mod output;
pub mod zip;
pub mod dedup;

pub fn dprint_imp(args: std::fmt::Arguments<'_>) {
    if util::DISP_ANSI.load(std::sync::atomic::Ordering::Relaxed) {
        let mut err = std::io::stderr().lock();
        err.write_fmt(format_args!("\x1B[2K\r")).unwrap();
        err.write_fmt(args).unwrap();
    } else {
        eprint!("{}",args);
    }
}

pub fn dprintln_imp(args: std::fmt::Arguments<'_>) {
    if util::DISP_ANSI.load(std::sync::atomic::Ordering::Relaxed) {
        let mut err = std::io::stderr().lock();
        err.write_fmt(format_args!("\x1B[2K\r")).unwrap();
        err.write_fmt(args).unwrap();
        err.write_all(b"\n").unwrap();
        print_statw(&mut err, false);
    } else {
        eprintln!("{}",args);
    }
}

pub fn print_statw(writer: &mut impl std::io::Write, force: bool) {
    use util::*;
    use std::sync::atomic::Ordering;
    use size_format::SizeFormatterBinary;

    if force || DISP_ENABLED.load(Ordering::Relaxed) {
        let processed_files = DISP_PROCESSED_FILES.load(Ordering::Relaxed);
        let relevant_files = DISP_RELEVANT_FILES.load(Ordering::Relaxed);
        let found_files = DISP_FOUND_FILES.load(Ordering::Relaxed);
        let processed_bytes = DISP_PROCESSED_BYTES.load(Ordering::Relaxed);
        let relevant_bytes = DISP_RELEVANT_BYTES.load(Ordering::Relaxed);
        let found_bytes = DISP_FOUND_BYTES.load(Ordering::Relaxed);
        let prev_bytes = DISP_PREV.swap(processed_bytes, Ordering::Relaxed).min(processed_bytes);
        let deduped_bytes = DISP_DEDUPED_BYTES.load(Ordering::Relaxed);
        let alloced = ALLOC_MON.load(Ordering::Relaxed) as u64;

        if deduped_bytes == u64::MAX {
            let _ = write!(
                writer,
                //"\n\x1B[2K\rAnalyzed files: {:>filefill$}/{} bytes: {:>12}B/{}B ({:>12}B/s) percent: {}%",
                "\x1B[2K\rFound: {} ({}B)        Hashed: {}/{} {}B/{}B ({}B/s)        alloc={}B ",
                found_files,
                SizeFormatterBinary::new(found_bytes),
                processed_files,
                relevant_files,
                SizeFormatterBinary::new(processed_bytes),
                SizeFormatterBinary::new(relevant_bytes),
                SizeFormatterBinary::new((processed_bytes - prev_bytes)*2), //TODO the "speed" needs to account for the actual Duration difference between prev and this call, as they aren't exclusively 500ms anymore
                SizeFormatterBinary::new(alloced),
                //( processed_bytes as f32 / relevant_bytes as f32 )*100.0,
                //filefill = relevant_files.to_string().len(),
            );
        }else{
            let _ = write!(
                writer,
                //"\n\x1B[2K\rAnalyzed files: {:>filefill$}/{} bytes: {:>12}B/{}B ({:>12}B/s) percent: {}%",
                "\x1B[2K\rDeduplication: Processed: {}/{} {}B/{}B ({}B/s)        Deduped: {}B ",
                processed_files,
                relevant_files,
                SizeFormatterBinary::new(processed_bytes),
                SizeFormatterBinary::new(relevant_bytes),
                SizeFormatterBinary::new((processed_bytes - prev_bytes)*2),
                SizeFormatterBinary::new(deduped_bytes as u64),
                //( processed_bytes as f32 / relevant_bytes as f32 )*100.0,
                //filefill = relevant_files.to_string().len(),
            );
        }
    }
    let _ = writer.flush();
}

pub fn print_stat_final() {
    let mut err = std::io::stderr().lock();
    //if util::DISP_ANSI.load(std::sync::atomic::Ordering::Relaxed) {
        print_statw(&mut err, true);
    //}
    err.write_all(b"\n").unwrap();
}

pub fn stat_section_start() {
    util::DISP_ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn stat_section_end() {
    if util::DISP_ENABLED.swap(false, std::sync::atomic::Ordering::Relaxed) {
        print_stat_final();
    }
}
