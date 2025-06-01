use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use std::{
    fs::File,
    io::{BufReader, Read, Write},
    sync::Arc,
};

use clap::Parser;
use crossbeam::channel::bounded;
use snaprs::{
    RAW_SAMP_RATE,
    payload::Payload,
    pipeline::pkt_wf,
    utils::{as_mut_u8_slice, slice_as_u8},
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short = 'i', long = "in", value_name = "input filename")]
    inname: String,

    #[clap(short = 'o', long = "out", value_name = "out name")]
    outname: Option<String>,

    #[clap(short = 'c', long = "nch", value_name = "num of ch")]
    nch: usize,

    #[clap(
        short = 'b',
        long = "nbatch",
        value_name = "fft batch",
        default_value_t = 1024
    )]
    nbatch: usize,

    #[clap(
        short = 'n',
        long = "nint",
        value_name = "num of fft per integration",
        default_value_t = 0
    )]
    nint: usize,
}

fn main() {
    //let (tx,rx)=bounded(256);
    let args = Args::parse();
    let nbatch = args.nbatch;
    let nint = if args.nint == 0 { nbatch } else { args.nint };

    let dt = (args.nch * 2 * args.nint) as f64 / RAW_SAMP_RATE as f64;
    let dt_per_iter = dt * (nbatch as f64 / nint as f64);
    println!("dt={dt} dt per int={dt_per_iter}");

    let (tx_payload, rx_payload) = bounded::<LinearOwnedReusable<Payload>>(65536);
    let (tx_payload1, rx_payload1) = bounded::<LinearOwnedReusable<Payload>>(65536);
    let (tx_wf, rx_wf) = bounded::<LinearOwnedReusable<Vec<f32>>>(4096);

    //let pool1 = Arc::clone(&pool);
    std::thread::spawn(move || pkt_wf(rx_payload1, tx_wf, args.nch, nbatch, nint));
    //std::thread::sleep(std::time::Duration::from_secs(1));
    //std::thread::spawn(|| recv_pkt(socket, tx_payload, rx_recv_cmd));
    let inname = args.inname;
    std::thread::spawn(move || {
        let mut bfrd = BufReader::new(File::open(&inname).unwrap());
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
        for payload_id in 0.. {
            let mut payload = pool.pull_owned();
            if let Ok(()) = bfrd.read_exact(as_mut_u8_slice(&mut payload.data)) {
                payload.pkt_cnt = payload_id;
                tx_payload.send(payload).unwrap();
            } else {
                break;
            }
        }
    });

    std::thread::spawn(move || {
        while let Ok(payload) = rx_payload.recv() {
            if tx_payload1.send(payload).is_err() {
                break;
            }
        }
    });

    //let mut dump_file = None;
    let mut outfile = args.outname.map(|outname| File::create(&outname).unwrap());
    for i in 0.. {
        if i % 100 == 0 {
            println!("{}", i);
        }
        if let Ok(x) = rx_wf.recv() {
            outfile.iter_mut().for_each(|f| {
                f.write_all(slice_as_u8(&x[..])).unwrap();
            });
        }
        break;
    }
}
