use std::{
    net::{SocketAddrV4, UdpSocket},
    thread::JoinHandle,
};

use crossbeam::channel::{bounded, Receiver, Sender};
use lockfree_object_pool::LinearOwnedReusable;
use num::Complex;


use crate::{
    ddc::{fir_coeffs_half, fir_coeffs_full, fir_coeffs_quarter, fir_coeffs_oct, N_PT_PER_FRAME},
    payload::Payload,
    pipeline::{pkt_ddc, recv_pkt, DdcCmd, RecvCmd},
};

#[derive(Debug, Clone, Copy)]
pub enum SdrSmpRate {
    SmpRate800,
    SmpRate400,
    SmpRate200,
    SmpRate100,
}

impl SdrSmpRate{
    pub fn to_ndec(&self) -> usize {
        match self {
            SdrSmpRate::SmpRate800 => 2,
            SdrSmpRate::SmpRate400 => 4,
            SdrSmpRate::SmpRate200 => 8,
            SdrSmpRate::SmpRate100 => 16,
        }
    }

    pub fn from_ndec(ndec: usize) -> SdrSmpRate {
        match ndec {
            2 => SdrSmpRate::SmpRate800,
            4 => SdrSmpRate::SmpRate400,
            8 => SdrSmpRate::SmpRate200,
            16 => SdrSmpRate::SmpRate100,
            _ => panic!("invalid ndec"),
        }
    }
}

pub struct Sdr {
    rx_thread: Option<JoinHandle<()>>,
    ddc_thread: Option<JoinHandle<()>>,
}

impl Drop for Sdr {
    fn drop(&mut self) {
        eprintln!("dropped");
        let h = self.ddc_thread.take();
        eprintln!("drop1");
        if let Some(h1) = h {
            if let Ok(()) = h1.join() {}
        }
        eprintln!("drop2");
        let h = self.rx_thread.take();
        if let Some(h1) = h {
            if let Ok(()) = h1.join() {}
        }
    }
}

impl Sdr {
    #[allow(clippy::type_complexity)]
    pub fn new(
        local_payload_addr: SocketAddrV4,
        smp_rate: SdrSmpRate,
    ) -> (
        Sdr,
        Receiver<LinearOwnedReusable<Vec<Complex<f32>>>>,
        Sender<DdcCmd>,
    ) {
        let payload_socket =
            UdpSocket::bind(local_payload_addr).expect("failed to bind payload socket");

        let (tx_payload, rx_payload) = bounded::<LinearOwnedReusable<Payload>>(65536);
        let (tx_ddc, rx_ddc) = bounded::<LinearOwnedReusable<Vec<Complex<f32>>>>(32768);
        let (tx_ddc_cmd, rx_ddc_cmd) = bounded::<DdcCmd>(32);
        let (tx_recv_cmd, rx_recv_cmd) = bounded::<RecvCmd>(32);

        tx_ddc_cmd
            .send(DdcCmd::LoCh(N_PT_PER_FRAME as isize / 4))
            .expect("failed to send loch");
        let rx_thread = std::thread::spawn(|| recv_pkt(payload_socket, tx_payload, rx_recv_cmd));
        let ddc_thread = std::thread::spawn(move || {
            let fir_coeffs = match smp_rate {
                SdrSmpRate::SmpRate800 => fir_coeffs_full(),
                SdrSmpRate::SmpRate400 => fir_coeffs_half(),
                SdrSmpRate::SmpRate200 => fir_coeffs_quarter(),
                SdrSmpRate::SmpRate100 => fir_coeffs_oct(),
            };
            pkt_ddc(rx_payload, tx_ddc, smp_rate.to_ndec(), rx_ddc_cmd, tx_recv_cmd, &fir_coeffs);
        });

        (
            Sdr {
                rx_thread: Some(rx_thread),
                ddc_thread: Some(ddc_thread),
            },
            rx_ddc,
            tx_ddc_cmd,
        )
    }
}
