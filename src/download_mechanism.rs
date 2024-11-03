use crate::{server::interception::SERVER_STATE, DownloadManager};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

trait ConsumingIterator<T> {
    fn next(&mut self) -> Option<T>;
}

impl<T> ConsumingIterator<T> for Vec<T> {
    fn next(&mut self) -> Option<T> {
        if !self.is_empty() {
            Some(self.remove(0))
        } else {
            None
        }
    }
}

pub fn check_urls(interface: &mut DownloadManager) {
    if let Ok(mut locked) = SERVER_STATE.try_lock() {
        let mut links = locked.clone();

        if !interface.popups.download.show {
            if let Some(link) = links.next() {
                interface.popups.download.link = link.clone();
                interface.popups.download.show = true;
                interface.show_window = true;

                *locked = links;
            }
        }
    }
}

pub fn set_total_bandwidth(interface: &mut DownloadManager) {
    let size: usize = interface
        .files
        .iter()
        .map(|f| {
            f.file
                .bytes_per_sec
                .load(std::sync::atomic::Ordering::Relaxed)
        })
        .sum();
    interface.bandwidth.total_bandwidth = size;
    update_bandwidth_history(interface);
}
fn update_bandwidth_history(interface: &mut DownloadManager) {
    interface
        .bandwidth
        .history
        .push(interface.bandwidth.total_bandwidth);
    if interface.bandwidth.history.len() > 100 {
        interface.bandwidth.history.remove(0);
    }
}
pub fn run_downloads(interface: &mut DownloadManager) {
    for fdl in interface.files.iter_mut() {
        let file = &fdl.file;
        let complete = file.complete.load(std::sync::atomic::Ordering::Relaxed);
        let new = fdl.new;
        let is_running = file.running.load(std::sync::atomic::Ordering::Relaxed);
        let speed = fdl
            .file
            .bytes_per_sec
            .load(std::sync::atomic::Ordering::Relaxed);

        fdl.has_error = speed == 0 && !complete && is_running;

        if !complete && !&fdl.initiated {
            let file = file.clone();
            let tx_error = interface.popups.error.channel.0.clone();
            interface.runtime.spawn(async move {
                if file.url.range_support {
                    loop {
                        match file.single_thread_dl().await {
                            Ok(_) => break,
                            Err(e) => {
                                let error = format!("{}: {:?}\n", file.name_on_disk, e);
                                tx_error.send(error).unwrap();
                            }
                        }
                        sleep(Duration::from_secs(5));
                    }
                } else if new {
                    match file.single_thread_dl().await {
                        Ok(_) => {}
                        Err(e) => {
                            let error = format!("{}: {:?}\n", file.name_on_disk, e);
                            tx_error.send(error).unwrap();
                        }
                    }
                }
            });

            fdl.initiated = true;
        }

        if let Ok(err) = interface.popups.error.channel.1.try_recv() {
            fdl.has_error = true;
            interface.popups.log.text.push_str(&err);
            interface.popups.log.has_error = true;
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub enum Actions {
    #[default]
    None,
    Reboot,
    Shutdown,
    Open,
}
