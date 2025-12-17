use std::fs::File;
use std::io::Write;
use std::thread;
use std::sync::Arc;
use uuid::Uuid;
use rand::RngCore;
use num_cpus;
use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::collections::HashSet;
use lazy_static::lazy_static;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
	// Parent directory for random files
	#[arg(short, long, default_value = ".")]
	parent_dir: PathBuf,

	// Randomized buffer size in MB
	#[arg(short, long, default_value = "100")]
	buffer_size: usize,
}

lazy_static! {
	static ref CREATED_FILES: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
}

static STOP_SIGNAL: AtomicBool = AtomicBool::new(false);

fn disk_thrash(parent_dir: &PathBuf, buffer: &[u8]) -> std::io::Result<()> {
	let filename = parent_dir.join(format!("{}.tmp", Uuid::new_v4()));

	{
		CREATED_FILES.lock().unwrap().insert(filename.clone());
	}

	let mut file = File::create(&filename)?;
	println!("Writing to file: {}", filename.display());

	// Check if buffer is empty
	if buffer.is_empty() {
		return Err(std::io::Error::new(
			std::io::ErrorKind::InvalidInput,
			"Buffer is empty",
		));
	}

	// Write the buffer to the file
	file.write_all(buffer)?;
	println!("Finished writing to file: {}", filename.display());

	// Ensure data is flushed to disk
	file.sync_all()?;
	println!("File sync complete for: {}", filename.display());

	let metadata = std::fs::metadata(&filename)?;
	if metadata.len() != buffer.len() as u64 {
		eprintln!(
			"Error: File did not write the expected size: {} bytes written, expected {} bytes.",
			metadata.len(),
			buffer.len()
		);
	}

	// Sleep before removal
	thread::sleep(std::time::Duration::from_secs(1));

	// Remove the file
	std::fs::remove_file(&filename)?;

	{
		CREATED_FILES.lock().unwrap().remove(&filename);
	}

	Ok(())
}

fn main() {
	let args = Args::parse();

	ctrlc::set_handler(|| {
		println!("CTRL+C received, stopping...");
		STOP_SIGNAL.store(true, Ordering::SeqCst);
	}).expect("Failed to set Ctrl-C handler");

	let size = args.buffer_size * 1024 * 1024;
	let mut buffer = vec![0u8; size];

	let mut rng = rand::rng();
	rng.fill_bytes(&mut buffer);

	let shared_buffer = Arc::new(buffer);
	let num_threads = num_cpus::get() - 2;

	println!("Spawning {} threads", num_threads);

	let mut handles = Vec::new();

	for id in 0..num_threads {
		let parent_dir = args.parent_dir.clone();
		let buffer = Arc::clone(&shared_buffer);

		handles.push(thread::spawn(move || {
			println!("Thread {} started", id);
			while !STOP_SIGNAL.load(Ordering::SeqCst) {
				if let Err(e) = disk_thrash(&parent_dir, &buffer) {
					eprintln!("Thread {} error: {}", id, e);
				}
			}
			println!("Thread {} stopping", id);
		}));
	}

	for h in handles {
		h.join().unwrap();
	}

	println!("Cleaning up remaining files...");

	let remaining: Vec<_> = CREATED_FILES.lock().unwrap().drain().collect();
	for path in remaining {
		let _ = std::fs::remove_file(path);
	}

	println!("Done.");
}
