// We will run a SPMC layout where a single producer produces passwords
// consumed by multiple workers. This ensures there is a buffer
// so the queue won't be consumed before the producer has time to wake up
const BUFFER_SIZE: usize = 200;

use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};

use crate::core::production::Producer;

use super::cracker::pdf::PDFCracker;
use tokio::sync::broadcast;
use tokio_util::task::TaskTracker;

async fn worker(
    cracker: Arc<PDFCracker>,
    success: broadcast::Sender<Vec<u8>>,
    mut source: broadcast::Receiver<Vec<u8>>,
) {
    while let Ok(passwd) = source.recv().await {
        if cracker.attempt(&passwd) {
            // inform main thread we found a good password then die
            success.send(passwd).unwrap_or_default();
            return;
        }
    }
}

async fn producer(
    producer: &mut Box<dyn Producer>,
    sender: broadcast::Sender<Vec<u8>>,
    progress: &ProgressBar,
) {
    while let Ok(x) = producer.next() {
        if let Some(password) = x {
            if let Err(_) = sender.send(password) {
                // This should only happen if their receiver is closed.
                error!("unable to send next password since channel is closed");
            }
            progress.inc(1);
        } else {
            trace!("Out of passwords.")
        }
    }
    todo!()
}

// Our async method allows us to select! on a password producer and our result channel :o

pub async fn crack_file(
    no_workers: usize,
    cracker: PDFCracker,
    producer: &mut Box<dyn Producer>,
) -> anyhow::Result<()> {
    // Open channels we use to send passwords to workers

    let sender = broadcast::Sender::<Vec<u8>>::new(BUFFER_SIZE);
    // Open channels that the workers will send their successes from
    let (success_sender, success_reader) = broadcast::channel::<Vec<u8>>(no_workers);
    let workers = TaskTracker::default();
    let cracker_handle = Arc::from(cracker);

    for _ in 0..no_workers {
        workers.spawn(worker(
            cracker_handle.clone(),
            success_sender.clone(),
            sender.subscribe(),
        ));
    }
    // We wont be starting more workers
    workers.close();
    // Drop our end of the password sender. We dont need it
    drop(success_sender);

    info!("Starting password cracking job...");


    let progress_bar = ProgressBar::new(producer.size() as u64);
    progress_bar.set_draw_delta(1000);
    progress_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {percent}% {per_sec} ETA: {eta}"));



    progress_bar.finish();
    let found_password = Some(vec![1,2,3]);
    
    match found_password {
        Some(password) => match std::str::from_utf8(&password) {
            Ok(password) => {
                info!("Success! Found password: {}", password)
            }
            Err(_) => {
                let hex_string: String = password
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<String>>()
                    .join(" ");
                info!(
                            "Success! Found password, but it contains invalid UTF-8 characters. Displaying as hex: {}",
                            hex_string
                        )
            }
        },
        None => {
            info!("Failed to crack file...")
        }
    }

    Ok(())
}
