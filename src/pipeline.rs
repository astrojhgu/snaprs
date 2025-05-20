use std::time::{Duration, Instant};
use std::{net::UdpSocket, sync::Arc};

use chrono::Local;
use crossbeam::channel::{Receiver, Sender};
use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use crate::{payload::Payload, utils::as_mut_u8_slice};

pub enum RecvCmd {
    Destroy,
}

pub fn recv_pkt(
    socket: UdpSocket,
    tx_payload: Sender<LinearOwnedReusable<Payload>>,
    rx_cmd: Receiver<RecvCmd>,
) {
    let mut last_print_time = Instant::now();
    let print_interval = Duration::from_secs(2);

    let mut next_cnt = None;
    let mut ndropped = 0;
    let pool: Arc<LinearObjectPool<Payload>> = Arc::new(LinearObjectPool::new(
        move || {
            //eprint!("o");
            Payload::default()
        },
        |v| {
            v.pkt_cnt = 0;
            v.data.fill(0);
        },
    ));
    //socket.set_nonblocking(true).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("failed to set timeout");
    loop {
        if !rx_cmd.is_empty() {
            match rx_cmd.recv().expect("failed to recv cmd") {
                RecvCmd::Destroy => break,
            }
        }
        let mut payload = pool.pull_owned();
        let buf = as_mut_u8_slice(&mut payload as &mut Payload);
        match socket.recv_from(buf) {
            Ok((s, _a)) => {
                if s != std::mem::size_of::<Payload>() {
                    continue;
                }
            }
            _ => continue,
        }

        let now = Instant::now();

        if now.duration_since(last_print_time) >= print_interval {
            let local_time = Local::now().format("%Y-%m-%d %H:%M:%S");
            println!(
                "{} {} pkts dropped q={}",
                local_time,
                ndropped,
                tx_payload.len()
            );
            last_print_time = now;
        }

        if next_cnt.is_none() {
            next_cnt = Some(payload.pkt_cnt);
            ndropped = 0;
        }

        if payload.pkt_cnt == 0 {
            ndropped = 0;
            let local_time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            println!();
            println!("==================================");
            println!("start time:{}", local_time);
            println!("==================================");
        }

        while let Some(ref mut c) = next_cnt {
            //let current_cnt = c + 1;
            if *c >= payload.pkt_cnt {
                //actually = is sufficient.
                *c = payload.pkt_cnt + 1;
                if tx_payload.is_full() {
                    //eprint!("O");
                    if !rx_cmd.is_empty() {
                        match rx_cmd.recv().expect("failed to recv cmd") {
                            RecvCmd::Destroy => return,
                        }
                    }
                    continue;
                }
                if let Ok(()) = tx_payload.send(payload) {
                    break;
                } else {
                    return;
                }
            }

            ndropped += 1;

            let mut payload1 = pool.pull_owned();
            payload1.copy_header(&payload);
            payload1.pkt_cnt = *c;
            if tx_payload.is_full() {
                //eprint!("O");
                if !rx_cmd.is_empty() {
                    match rx_cmd.recv().expect("failed to recv cmd") {
                        RecvCmd::Destroy => return,
                    }
                }
                continue;
            }
            if tx_payload.send(payload1).is_err() {
                return;
            }
            *c += 1;
        }
    }
}

pub fn pkt_wf(
    rx: Receiver<LinearOwnedReusable<Payload>>,
    tx: Sender<LinearOwnedReusable<Vec<f32>>>,
    nch: usize,
    nbatch: usize,
    nint: usize,
) {
    assert_eq!(nbatch % nint, 0);
    use crate::cuwf::WfResource;
    let mut wf = WfResource::new(nch, nbatch, nint);
    let nbuf = nch * nbatch / nint;

    let pool: Arc<LinearObjectPool<Vec<f32>>> = Arc::new(LinearObjectPool::new(
        move || {
            //eprint!(".");
            vec![0_f32; nbuf]
        },
        |_v| {},
    ));
    let mut result = pool.pull_owned();
    while let Ok(payload) = rx.recv() {
        if wf.process(&payload.data, result.as_mut_slice()) {
            if tx.is_full() {
                eprintln!("waterfall channel full, discarding");
                continue;
            }
            if tx.send(result).is_err() {
                break;
            }
            result = pool.pull_owned();
        }
    }
}
