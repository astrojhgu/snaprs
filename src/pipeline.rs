use std::time::{Duration, Instant};
use std::{net::UdpSocket, sync::Arc};

use chrono::Local;
use crossbeam::channel::{Receiver, Sender};
use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use rustfft::num_complex::Complex;

use crate::ddc::N_PT_PER_FRAME;
use crate::{payload::Payload, utils::as_mut_u8_slice, ddc::DownConverter};

pub enum RecvCmd {
    Destroy,
}

pub fn fake_dev(tx_payload: Sender<LinearOwnedReusable<Payload>>, rx_cmd: Receiver<RecvCmd>) {
    let mut last_print_time = Instant::now();
    let t0= Instant::now();
    let print_interval = Duration::from_secs(2);

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
    for pkt_cnt in 0.. {
        if !rx_cmd.is_empty() {
            match rx_cmd.recv().expect("failed to recv cmd") {
                RecvCmd::Destroy => break,
            }
        }
        let mut payload = pool.pull_owned();
        payload.pkt_cnt = pkt_cnt;

        let now = Instant::now();

        if payload.pkt_cnt == 0 {
            let local_time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            println!();
            println!("==================================");
            println!("start time:{}", local_time);
            println!("==================================");
        } else if now.duration_since(last_print_time) >= print_interval {
            let dt=now.duration_since(t0).as_secs_f64();
            let npkts=pkt_cnt as usize;
            let nsamp=npkts*N_PT_PER_FRAME;
            let smp_rate=nsamp as f64/dt;
            println!("smp_rate: {} MSps q={}", smp_rate/1e6, tx_payload.len());
            last_print_time = now;
        }

        if tx_payload.send(payload).is_err() {
            return;
        }
    }
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

#[derive(Debug, Copy, Clone)]
pub enum DdcCmd {
    LoCh(isize),
    Destroy,
}


pub fn pkt_ddc(
    rx: Receiver<LinearOwnedReusable<Payload>>,
    tx: Sender<LinearOwnedReusable<Vec<Complex<f32>>>>,
    ndec: usize,
    rx_ddc_cmd: Receiver<DdcCmd>,
    tx_recv_cmd: Sender<RecvCmd>,
    fir_coeffs: &[f32],
) {
    let mut ddc = DownConverter::new(ndec, fir_coeffs);
    let n_out_data = ddc.n_out_data();
    let pool: Arc<LinearObjectPool<Vec<Complex<f32>>>> = Arc::new(LinearObjectPool::new(
        move || {
            //eprint!(".");
            vec![Complex::<f32>::default(); n_out_data]
        },
        |_v| {},
    ));

    let mut lo_ch = if let DdcCmd::LoCh(c) = rx_ddc_cmd.recv().expect("failed to recv cmd") {
        c
    } else {
        N_PT_PER_FRAME as isize / 4
    };

    loop {
        if !rx_ddc_cmd.is_empty() {
            if let Ok(x) = rx_ddc_cmd.recv() {
                match x {
                    DdcCmd::LoCh(c) => {
                        lo_ch = c;
                    }
                    DdcCmd::Destroy => {
                        break;
                    }
                }
            } else {
                break;
            }
        }
        if let Ok(payload) = rx.recv_timeout(Duration::from_secs(1)) {
            if ddc.ddc(&payload.data, lo_ch) {
                let mut outdata = pool.pull_owned();
                ddc.fetch_output(&mut outdata);

                if tx.is_full() {
                    eprintln!("ddc channel full, discarding");
                    continue;
                }
                if tx.send_timeout(outdata, Duration::from_secs(1)).is_err() {
                    break;
                }
            }
        }
    }
    drop(rx);
    tx_recv_cmd
        .send(RecvCmd::Destroy)
        .expect("failed to send cmd");
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
