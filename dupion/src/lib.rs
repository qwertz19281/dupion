use anyhow::anyhow;
use anyhow::bail;
use anyhow::ensure;
use anyhow::Result as AnyhowResult;

#[macro_export]
macro_rules! dprint {
    ($($arg:tt)*) => {{
        let mut err = std::io::stderr().lock();
        std::io::Write::write_fmt(&mut err, format_args!("\x1B[2K\r")).unwrap();
        std::io::Write::write_fmt(&mut err, format_args!($($arg)*)).unwrap();
    }};
}

#[macro_export]
macro_rules! dprintln {
    ($($arg:tt)*) => {{
        let mut err = std::io::stderr().lock();
        std::io::Write::write_fmt(&mut err, format_args!("\x1B[2K\r")).unwrap();
        std::io::Write::write_fmt(&mut err, format_args!($($arg)*)).unwrap();
        std::io::Write::write_all(&mut err, b"\n").unwrap();
        $crate::print_statw(&mut err, false);
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
                SizeFormatterBinary::new((processed_bytes - prev_bytes)*2),
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

pub fn print_stat() {
    let mut err = std::io::stderr().lock();
    print_statw(&mut err, true);
}
