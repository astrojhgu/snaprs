use std::{fs::File, io::Write};

use clap::Parser;
use num::Complex;

use snaprs::{ddc::N_PT_PER_FRAME, sdr::{Sdr, SdrSmpRate}, utils::slice_as_u8};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short = 'a', value_name = "local payload ip:port")]
    local_payload_addr: String,


    #[clap(short = 'o', long = "out", value_name = "out name")]
    outname: Option<String>,

    #[clap(short = 'l', value_name = "loch", default_value_t=(N_PT_PER_FRAME/4).try_into().unwrap())]
    lo_ch: isize,

    #[clap(short = 'N', value_name = "Num of samples in 10^6")]
    nsamp: Option<usize>,
}

fn main() {
    //let (tx,rx)=bounded(256);

    use snaprs::pipeline::DdcCmd;

    let args = Args::parse();

    let (sdr, rx_ddc, tx_cmd) = Sdr::new(
       args.local_payload_addr
            .parse()
            .expect("failed to parse local payload addr"),
        SdrSmpRate::from_ndec(16),
    );

    let mut dump_file = args
        .outname
        .map(|outname| File::create(&outname).expect("failed to create dump file"));
    let mut _bytes_written = 0;
    tx_cmd
        .send(snaprs::pipeline::DdcCmd::LoCh(args.lo_ch))
        .expect("failed to send cmd");
    
    let mut nsamp: Option<usize> = args.nsamp.map(|x| x * 1_000_000);
    for _i in 0.. {
        let ddc = rx_ddc.recv().expect("failed to recv ddc payload");
        println!("{}", ddc.len());

        let n_to_write = if let Some(n) = nsamp {
            n.min(ddc.len())
        } else {
            ddc.len()
        };

        if n_to_write == 0 {
            break;
        }

        nsamp.iter_mut().for_each(|x| {
            *x -= n_to_write;
        });
        if let Some(ref mut f) = dump_file {
            //dump_file = Some(File::create(outname).unwrap());
            f.write_all(slice_as_u8(&ddc[..n_to_write]))
                .expect("failed to write");
            _bytes_written += ddc.len() * std::mem::size_of::<Complex<f32>>();
            //println!("{} MBytes written", bytes_written as f64 / 1e6);
        }
    }
    tx_cmd
        .send(DdcCmd::Destroy)
        .expect("failed to send destroy command");
    drop(rx_ddc);
}

