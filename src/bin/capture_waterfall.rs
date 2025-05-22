use lockfree_object_pool::LinearOwnedReusable;
use std::{fs::File, io::BufWriter, io::Write, net::UdpSocket};

use clap::Parser;
use crossbeam::channel::bounded;
use snaprs::{
    payload::Payload,
    pipeline::{pkt_wf, recv_pkt},
    utils::slice_as_u8,
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short = 'a', long = "addr", value_name = "ip:port")]
    local_addr: String,

    #[clap(short = 'o', long = "out", value_name = "out name")]
    outname: Option<String>,

    #[clap(short = 'r', long = "raw", value_name = "out name")]
    raw_outname: Option<String>,

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

    let socket = UdpSocket::bind(&args.local_addr).unwrap();
    let (tx_payload, rx_payload) = bounded::<LinearOwnedReusable<Payload>>(65536);
    let (tx_payload1, rx_payload1) = bounded::<LinearOwnedReusable<Payload>>(65536);
    let (tx_wf, rx_wf) = bounded::<LinearOwnedReusable<Vec<f32>>>(4096);
    let (_tx_recv_cmd, rx_recv_cmd) = bounded(1024);
    //let pool1 = Arc::clone(&pool);
    std::thread::spawn(move || pkt_wf(rx_payload1, tx_wf, args.nch, nbatch, nint));
    //std::thread::sleep(std::time::Duration::from_secs(1));
    std::thread::spawn(|| recv_pkt(socket, tx_payload, rx_recv_cmd));
    let raw_outname = args.raw_outname.clone();
    std::thread::spawn(move || {
        if let Some(f) = raw_outname.map(|on| File::create(&on).unwrap()) {
            let mut bfw = BufWriter::new(f);
            while let Ok(payload) = rx_payload.recv() {
                bfw.write_all(slice_as_u8(&payload.data)).unwrap();
                if tx_payload1.send(payload).is_err() {
                    break;
                }
            }
        } else {
            while let Ok(payload) = rx_payload.recv() {
                if tx_payload1.send(payload).is_err() {
                    break;
                }
            }
        }
    });

    //let mut dump_file = None;
    let mut outfile = args.outname.map(|outname| File::create(&outname).unwrap());
    for _i in 0.. {
        let x = rx_wf.recv().unwrap();
        outfile.iter_mut().for_each(|f| {
            f.write_all(slice_as_u8(&x[..])).unwrap();
        });
    }
}
