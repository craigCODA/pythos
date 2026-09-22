//! Finite, capability-gated TLS 1.3 acceptance consumer.
//!
//! This module is deliberately private to the socket probe. It adapts the
//! existing copied-frame NetworkPort and TCP proof to embedded-tls; it does
//! not add a PythOS socket or secure-transport ABI.

use core::{cell::UnsafeCell, fmt};

use embedded_io::{Error as EmbeddedError, ErrorKind, ErrorType, Read, Write};
use embedded_tls::blocking::{
    Aes128GcmSha256, Certificate, CertificateEntryRef, CertificateRef, CryptoProvider, NoClock,
    TlsCipherSuite, TlsConfig, TlsContext, TlsError, TlsVerifier,
};
use embedded_tls::pki::CertVerifier;
use pythos_shared::capability_abi::PackedCapability;
use rand_core::{CryptoRng, RngCore};

use pythos_user_arp_probe::arp::parse as parse_arp;
use pythos_user_ipv4_probe::ipv4::{Ipv4Header, decode_ipv4_packet, encode_ipv4_packet};
use pythos_user_link_layer_probe::ethernet;
use pythos_user_tcp_probe::tcp::{
    TCP_FLAG_ACK, TCP_FLAG_FIN_ACK, TCP_FLAG_SYN, TCP_FLAG_SYN_ACK, TCP_HEADER_BYTES, TCP_MSS,
    TCP_PROTOCOL, TCP_SYN_HEADER_BYTES, TcpSegment, decode_stream_segment, encode_stream_segment,
};

use super::{
    NETWORK_PORT_MAX_FRAME_BYTES, NETWORK_PORT_MIN_FRAME_BYTES, STORAGE, arp_reply_matches,
    arp_request_frame, describe, error, send_frame, send_frame_bytes, valid_bootstrap,
    valid_bootstrap_header, valid_description, write_marker,
};

const STREAM_TCP_PAYLOAD_BYTES: usize = 1024;
const STREAM_TCP_BYTES: usize = TCP_HEADER_BYTES + STREAM_TCP_PAYLOAD_BYTES;
const STREAM_TCP_SYN_BYTES: usize = TCP_SYN_HEADER_BYTES;
const STREAM_IP_BYTES: usize = 20 + STREAM_TCP_BYTES;
const STREAM_RX_BYTES: usize = NETWORK_PORT_MAX_FRAME_BYTES - 54;
const TLS_READ_BUFFER_BYTES: usize = 4096;
const TLS_WRITE_BUFFER_BYTES: usize = 2048;
const MAX_RECEIVE_POLL_ATTEMPTS: usize = 1024;
const TLS_SERVER_NAME: &str = "pythos.test";
const SECURE_REQUEST: &[u8] = b"PYTHOS-SECURE-REQUEST";
const SECURE_RESPONSE: &[u8] = b"PYTHOS-SECURE-RESPONSE";

#[repr(C)]
struct SecureTlsBuffers {
    read: [u8; TLS_READ_BUFFER_BYTES],
    write: [u8; TLS_WRITE_BUFFER_BYTES],
}

struct SecureTlsStorage(UnsafeCell<SecureTlsBuffers>);

// SAFETY: this finite proof has one single-threaded secure consumer and the
// storage is borrowed only for the duration of that consumer's TLS session.
unsafe impl Sync for SecureTlsStorage {}

static SECURE_TLS_STORAGE: SecureTlsStorage = SecureTlsStorage(UnsafeCell::new(SecureTlsBuffers {
    read: [0; TLS_READ_BUFFER_BYTES],
    write: [0; TLS_WRITE_BUFFER_BYTES],
}));

#[repr(C)]
struct SecureFrameBuffers {
    tcp: [u8; STREAM_TCP_BYTES],
    ip: [u8; STREAM_IP_BYTES],
    frame: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
}

struct SecureFrameStorage(UnsafeCell<SecureFrameBuffers>);

// SAFETY: the same single-threaded proof owns this scratch storage for each
// bounded frame construction, and `send_frame_bytes` copies before returning.
unsafe impl Sync for SecureFrameStorage {}

static SECURE_FRAME_STORAGE: SecureFrameStorage =
    SecureFrameStorage(UnsafeCell::new(SecureFrameBuffers {
        tcp: [0; STREAM_TCP_BYTES],
        ip: [0; STREAM_IP_BYTES],
        frame: [0; NETWORK_PORT_MAX_FRAME_BYTES],
    }));

struct SecureReceiveFrameStorage(UnsafeCell<[u8; NETWORK_PORT_MAX_FRAME_BYTES]>);

// SAFETY: the finite proof has one serialized receive path and consumes the
// frame before issuing the next receive operation.
unsafe impl Sync for SecureReceiveFrameStorage {}

static SECURE_RECEIVE_FRAME: SecureReceiveFrameStorage =
    SecureReceiveFrameStorage(UnsafeCell::new([0; NETWORK_PORT_MAX_FRAME_BYTES]));

struct SecureSegmentDataStorage(UnsafeCell<[u8; STREAM_RX_BYTES]>);

// SAFETY: parsed segment data is consumed synchronously before the next frame
// is parsed, with no concurrent consumer.
unsafe impl Sync for SecureSegmentDataStorage {}

static SECURE_SEGMENT_DATA: SecureSegmentDataStorage =
    SecureSegmentDataStorage(UnsafeCell::new([0; STREAM_RX_BYTES]));

const ARP_ETHER_TYPE: u16 = 0x0806;
const IPV4_ETHER_TYPE: u16 = 0x0800;
const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const LOCAL_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
const PEER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const LOCAL_IPV4: [u8; 4] = [192, 168, 14, 2];
const PEER_IPV4: [u8; 4] = [192, 168, 14, 1];
const TCP_SOURCE_PORT: u16 = 0x1505;
const TCP_DESTINATION_PORT: u16 = 0x1506;
const TCP_WINDOW: u16 = 0x1000;
const TCP_LOCAL_ISS: u32 = 0x1505_0000;
const TCP_PEER_ISS: u32 = 0x2506_0000;

// DER certificate for the fixed pythos.test acceptance peer.
// SHA-256: 05fbe163a52218a9f419c17b540e73b963f9a265a43b7bc05587934b07ea7a4e
const SERVER_CERT_DER: [u8; 311] = [
    0x30, 0x82, 0x01, 0x33, 0x30, 0x81, 0xdb, 0xa0, 0x03, 0x02, 0x01, 0x02, 0x02, 0x01, 0x01, 0x30,
    0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02, 0x30, 0x16, 0x31, 0x14, 0x30,
    0x12, 0x06, 0x03, 0x55, 0x04, 0x03, 0x0c, 0x0b, 0x70, 0x79, 0x74, 0x68, 0x6f, 0x73, 0x2e, 0x74,
    0x65, 0x73, 0x74, 0x30, 0x1e, 0x17, 0x0d, 0x32, 0x30, 0x30, 0x31, 0x30, 0x31, 0x30, 0x30, 0x30,
    0x30, 0x30, 0x30, 0x5a, 0x17, 0x0d, 0x33, 0x30, 0x30, 0x31, 0x30, 0x31, 0x30, 0x30, 0x30, 0x30,
    0x30, 0x30, 0x5a, 0x30, 0x16, 0x31, 0x14, 0x30, 0x12, 0x06, 0x03, 0x55, 0x04, 0x03, 0x0c, 0x0b,
    0x70, 0x79, 0x74, 0x68, 0x6f, 0x73, 0x2e, 0x74, 0x65, 0x73, 0x74, 0x30, 0x59, 0x30, 0x13, 0x06,
    0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03,
    0x01, 0x07, 0x03, 0x42, 0x00, 0x04, 0xcd, 0x29, 0x34, 0xf1, 0x1a, 0xcd, 0xaf, 0x34, 0xf6, 0x0e,
    0xde, 0xde, 0x14, 0x63, 0x6b, 0x11, 0x68, 0x8c, 0x2c, 0x6b, 0x3c, 0x74, 0x80, 0x62, 0x9f, 0x90,
    0x4c, 0x3b, 0xbb, 0x7e, 0x72, 0x24, 0xea, 0x97, 0x4e, 0xd9, 0x45, 0x86, 0xc5, 0x7b, 0x64, 0x18,
    0x69, 0x67, 0x07, 0x78, 0x40, 0x39, 0xed, 0xef, 0xa7, 0xff, 0xe9, 0x1a, 0x3a, 0x25, 0xcd, 0x48,
    0x27, 0xbe, 0xf7, 0xb0, 0xff, 0xb1, 0xa3, 0x1a, 0x30, 0x18, 0x30, 0x16, 0x06, 0x03, 0x55, 0x1d,
    0x11, 0x04, 0x0f, 0x30, 0x0d, 0x82, 0x0b, 0x70, 0x79, 0x74, 0x68, 0x6f, 0x73, 0x2e, 0x74, 0x65,
    0x73, 0x74, 0x30, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02, 0x03, 0x47,
    0x00, 0x30, 0x44, 0x02, 0x20, 0x22, 0xc2, 0x58, 0xff, 0x8e, 0xfd, 0xa8, 0x1c, 0x57, 0xa9, 0x74,
    0x35, 0x4c, 0x7e, 0xc3, 0x4c, 0xe0, 0x5a, 0x1f, 0x07, 0xa6, 0x79, 0x68, 0xe1, 0x6d, 0x17, 0x04,
    0x79, 0x3a, 0x49, 0xf3, 0xf0, 0x02, 0x20, 0x35, 0xc4, 0x2a, 0x64, 0x97, 0x7f, 0x58, 0x30, 0x97,
    0x59, 0x6e, 0x7d, 0x63, 0xda, 0x4d, 0x7d, 0x05, 0x76, 0x41, 0x6e, 0x22, 0x25, 0xa8, 0x88, 0xae,
    0xa1, 0x10, 0xe2, 0xd7, 0xe1, 0xe3, 0x6c,
];

pub(super) fn start(bootstrap_ptr: u64, console_raw: u64) -> ! {
    let console = PackedCapability::from_raw(console_raw);
    // SAFETY: PythCore supplies one aligned bootstrap page for this probe.
    let bootstrap = unsafe {
        (bootstrap_ptr as *const pythos_shared::network_port_abi::NetworkPortBootstrapV1).read()
    };
    if !valid_bootstrap_header(bootstrap) {
        error(console);
    }
    if bootstrap.port_capability.raw() == 0 {
        write_marker(
            console,
            pythos_shared::secure_transport_markers::SECURE_DENIED_BOOTSTRAPPED_MARKER,
        );
        write_marker(
            console,
            pythos_shared::secure_transport_markers::SECURE_OPEN_WITHOUT_CAP_DENIED_MARKER,
        );
        write_marker(
            console,
            pythos_shared::secure_transport_markers::SECURE_DENIED_TEARDOWN_COMPLETE_MARKER,
        );
        super::success_breakpoint();
    }
    if !valid_bootstrap(bootstrap) {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_BOOTSTRAPPED_MARKER,
    );

    let capability = bootstrap.port_capability;
    if describe(capability) != pythos_shared::network_port_abi::NETWORK_PORT_STATUS_OK
        || !valid_description()
    {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_OPEN_GRANTED_MARKER,
    );

    let mut stream = TcpStream::new(capability);
    if stream.connect().is_err() {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_TCP_READY_MARKER,
    );

    let verifier = PinnedVerifier::new(&SERVER_CERT_DER);
    let provider = TestProvider::new(verifier);
    let config = TlsConfig::new().with_server_name(TLS_SERVER_NAME);
    // SAFETY: the opt-in proof launches exactly one secure consumer on one
    // core, and no other path can borrow this private storage concurrently.
    let tls_buffers = unsafe { &mut *SECURE_TLS_STORAGE.0.get() };
    let mut tls = embedded_tls::blocking::TlsConnection::new(
        stream,
        &mut tls_buffers.read,
        &mut tls_buffers.write,
    );
    if tls.open(TlsContext::new(&config, provider)).is_err() {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_TLS_HANDSHAKE_OK_MARKER,
    );

    if tls.write(SECURE_REQUEST).is_err() || tls.flush().is_err() {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_REQUEST_ENCRYPTED_MARKER,
    );

    let mut response = [0u8; SECURE_RESPONSE.len()];
    let mut received = 0;
    while received < response.len() {
        match tls.read(&mut response[received..]) {
            Ok(0) => error(console),
            Ok(count) => received += count,
            Err(_) => {
                response.fill(0);
                write_marker(
                    console,
                    pythos_shared::secure_transport_markers::SECURE_TAMPER_REJECTED_MARKER,
                );
                let mut stream = match tls.close() {
                    Ok(stream) => stream,
                    Err((stream, _error)) => stream,
                };
                let _ = stream.close_tcp();
                super::success_breakpoint();
            }
        }
    }
    if response != SECURE_RESPONSE {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_RESPONSE_DECRYPTED_MARKER,
    );

    let mut stream = match tls.close() {
        Ok(stream) => stream,
        Err((_stream, _error)) => error(console),
    };
    if stream.close_tcp().is_err() {
        error(console);
    }
    write_marker(
        console,
        pythos_shared::secure_transport_markers::SECURE_CLOSE_OK_MARKER,
    );
    super::success_breakpoint();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FrameError;

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NetworkPort frame exchange failed")
    }
}

impl core::error::Error for FrameError {}

impl EmbeddedError for FrameError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

struct TestRng(u64);

impl RngCore for TestRng {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_u64(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        for chunk in destination.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            let count = chunk.len();
            chunk.copy_from_slice(&bytes[..count]);
        }
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(destination);
        Ok(())
    }
}

impl CryptoRng for TestRng {}

struct PinnedVerifier<'a> {
    inner: CertVerifier<'a, Aes128GcmSha256, NoClock, 2048>,
    pinned: &'a [u8],
}

impl<'a> PinnedVerifier<'a> {
    fn new(pinned: &'a [u8]) -> Self {
        Self {
            inner: CertVerifier::new(Certificate::X509(pinned)),
            pinned,
        }
    }
}

impl TlsVerifier<Aes128GcmSha256> for PinnedVerifier<'_> {
    fn set_hostname_verification(&mut self, hostname: &str) -> Result<(), TlsError> {
        self.inner.set_hostname_verification(hostname)
    }

    fn verify_certificate(
        &mut self,
        transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        certificate: CertificateRef,
    ) -> Result<(), TlsError> {
        match certificate.entries.first() {
            Some(CertificateEntryRef::X509(der)) if *der == self.pinned => {}
            _ => return Err(TlsError::InvalidCertificate),
        }
        self.inner.verify_certificate(transcript, certificate)
    }

    fn verify_signature(
        &mut self,
        verify: embedded_tls::blocking::CertificateVerifyRef,
    ) -> Result<(), TlsError> {
        self.inner.verify_signature(verify)
    }
}

struct TestProvider<'a> {
    rng: TestRng,
    verifier: PinnedVerifier<'a>,
}

impl<'a> TestProvider<'a> {
    fn new(verifier: PinnedVerifier<'a>) -> Self {
        Self {
            rng: TestRng(0x5059_5448_4f53_3134),
            verifier,
        }
    }
}

impl CryptoProvider for TestProvider<'_> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = [u8; 0];

    fn rng(&mut self) -> impl rand_core::CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

#[derive(Clone, Copy)]
struct ReceivedSegment {
    sequence: u32,
    acknowledgment: u32,
    flags: u8,
    data_len: usize,
}

struct TcpStream {
    capability: PackedCapability,
    local_sequence: u32,
    peer_sequence: u32,
    ip_identification: u16,
    receive: [u8; STREAM_RX_BYTES],
    receive_start: usize,
    receive_len: usize,
    peer_fin: bool,
}

impl TcpStream {
    fn new(capability: PackedCapability) -> Self {
        Self {
            capability,
            local_sequence: TCP_LOCAL_ISS,
            peer_sequence: 0,
            ip_identification: 0x1601,
            receive: [0; STREAM_RX_BYTES],
            receive_start: 0,
            receive_len: 0,
            peer_fin: false,
        }
    }

    fn connect(&mut self) -> Result<(), FrameError> {
        send_frame(self.capability, arp_request_frame()).map_err(|_| FrameError)?;
        let arp_len = self.receive_frame()?;
        // SAFETY: `receive_frame` filled this private serialized receive
        // buffer and no concurrent consumer can mutate it here.
        let arp_frame = unsafe { &*SECURE_RECEIVE_FRAME.0.get() };
        if !arp_reply_matches(&arp_frame[..arp_len], 0) {
            return Err(FrameError);
        }

        self.send_segment(TCP_FLAG_SYN, TCP_LOCAL_ISS, 0, Some(TCP_MSS), &[])?;
        let peer = self.receive_segment()?;
        if peer.flags != TCP_FLAG_SYN_ACK
            || peer.sequence != TCP_PEER_ISS
            || peer.acknowledgment != TCP_LOCAL_ISS + 1
            || peer.data_len != 0
        {
            return Err(FrameError);
        }
        self.local_sequence = TCP_LOCAL_ISS + 1;
        self.peer_sequence = TCP_PEER_ISS + 1;
        self.send_ack()
    }

    fn close_tcp(&mut self) -> Result<(), FrameError> {
        let sequence = self.local_sequence;
        self.send_segment(TCP_FLAG_FIN_ACK, sequence, self.peer_sequence, None, &[])?;
        self.local_sequence = self.local_sequence.checked_add(1).ok_or(FrameError)?;
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS {
            let peer = self.receive_segment()?;
            if peer.data_len != 0 {
                self.accept_peer_data(peer)?;
            }
            if peer.flags & pythos_user_tcp_probe::tcp::TCP_FLAG_FIN != 0 {
                if peer.sequence != self.peer_sequence {
                    return Err(FrameError);
                }
                self.peer_sequence = self.peer_sequence.checked_add(1).ok_or(FrameError)?;
                self.peer_fin = true;
                self.send_ack()?;
                return Ok(());
            }
            if peer.acknowledgment >= self.local_sequence && self.peer_fin {
                return Ok(());
            }
        }
        Err(FrameError)
    }

    fn send_ack(&mut self) -> Result<(), FrameError> {
        self.send_segment(
            TCP_FLAG_ACK,
            self.local_sequence,
            self.peer_sequence,
            None,
            &[],
        )
    }

    fn send_segment(
        &mut self,
        flags: u8,
        sequence: u32,
        acknowledgment: u32,
        mss: Option<u16>,
        data: &[u8],
    ) -> Result<(), FrameError> {
        if data.len() > STREAM_TCP_PAYLOAD_BYTES {
            return Err(FrameError);
        }
        let tcp_len = if mss.is_some() {
            STREAM_TCP_SYN_BYTES
        } else {
            TCP_HEADER_BYTES + data.len()
        };
        // SAFETY: this proof has one active sender and the frame is copied by
        // `send_frame_bytes` before the scratch storage is reused.
        let buffers = unsafe { &mut *SECURE_FRAME_STORAGE.0.get() };
        buffers.tcp.fill(0);
        let segment = TcpSegment {
            source_port: TCP_SOURCE_PORT,
            destination_port: TCP_DESTINATION_PORT,
            sequence,
            acknowledgment,
            flags,
            window: TCP_WINDOW,
            urgent_pointer: 0,
            mss,
            data,
        };
        encode_stream_segment(segment, LOCAL_IPV4, PEER_IPV4, &mut buffers.tcp[..tcp_len])
            .map_err(|_| FrameError)?;

        let ip_len = 20 + tcp_len;
        buffers.ip.fill(0);
        let header = Ipv4Header {
            version: 4,
            ihl: 5,
            dscp_ecn: 0,
            identification: self.ip_identification,
            flags_fragment_offset: 0,
            ttl: 64,
            protocol: TCP_PROTOCOL,
            source: LOCAL_IPV4,
            destination: PEER_IPV4,
        };
        self.ip_identification = self.ip_identification.wrapping_add(1);
        encode_ipv4_packet(header, &buffers.tcp[..tcp_len], &mut buffers.ip[..ip_len])
            .map_err(|_| FrameError)?;

        let ethernet_len = 14 + ip_len;
        let frame_len = core::cmp::max(NETWORK_PORT_MIN_FRAME_BYTES, ethernet_len);
        buffers.frame.fill(0);
        buffers.frame[0..6].copy_from_slice(&PEER_MAC);
        buffers.frame[6..12].copy_from_slice(&LOCAL_MAC);
        buffers.frame[12..14].copy_from_slice(&IPV4_ETHER_TYPE.to_be_bytes());
        buffers.frame[14..ethernet_len].copy_from_slice(&buffers.ip[..ip_len]);
        send_frame_bytes(self.capability, &buffers.frame[..frame_len]).map_err(|_| FrameError)
    }

    fn receive_frame(&mut self) -> Result<usize, FrameError> {
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS {
            let status = super::receive(self.capability);
            if status == pythos_shared::network_port_abi::NETWORK_PORT_STATUS_EMPTY {
                core::hint::spin_loop();
                continue;
            }
            if status != pythos_shared::network_port_abi::NETWORK_PORT_STATUS_OK {
                return Err(FrameError);
            }
            // SAFETY: the parent probe serializes all NetworkPort requests.
            let buffers = unsafe { &*STORAGE.0.get() };
            let frame_len = buffers.response.frame_len as usize;
            if !(NETWORK_PORT_MIN_FRAME_BYTES..=NETWORK_PORT_MAX_FRAME_BYTES).contains(&frame_len) {
                return Err(FrameError);
            }
            // SAFETY: the parent probe serializes all NetworkPort requests and
            // this method returns before the buffer is reused.
            let frame = unsafe { &mut *SECURE_RECEIVE_FRAME.0.get() };
            frame[..frame_len].copy_from_slice(&buffers.rx[..frame_len]);
            return Ok(frame_len);
        }
        Err(FrameError)
    }

    fn receive_segment(&mut self) -> Result<ReceivedSegment, FrameError> {
        let frame_len = self.receive_frame()?;
        // SAFETY: `receive_frame` populated this private serialized buffer.
        let frame = unsafe { &*SECURE_RECEIVE_FRAME.0.get() };
        let ethernet = ethernet::parse(&frame[..frame_len]).map_err(|_| FrameError)?;
        if ethernet.destination != LOCAL_MAC
            || ethernet.source != PEER_MAC
            || ethernet.ether_type != IPV4_ETHER_TYPE
        {
            return Err(FrameError);
        }
        let packet = decode_ipv4_packet(ethernet.payload).map_err(|_| FrameError)?;
        if packet.header.source != PEER_IPV4
            || packet.header.destination != LOCAL_IPV4
            || packet.header.protocol != TCP_PROTOCOL
        {
            return Err(FrameError);
        }
        let segment =
            decode_stream_segment(packet.payload, PEER_IPV4, LOCAL_IPV4).map_err(|_| FrameError)?;
        if segment.source_port != TCP_DESTINATION_PORT
            || segment.destination_port != TCP_SOURCE_PORT
        {
            return Err(FrameError);
        }
        if segment.data.len() > STREAM_RX_BYTES {
            return Err(FrameError);
        }
        // SAFETY: this parsed segment is consumed synchronously before the
        // next receive, with no concurrent consumer.
        let data = unsafe { &mut *SECURE_SEGMENT_DATA.0.get() };
        data[..segment.data.len()].copy_from_slice(segment.data);
        Ok(ReceivedSegment {
            sequence: segment.sequence,
            acknowledgment: segment.acknowledgment,
            flags: segment.flags,
            data_len: segment.data.len(),
        })
    }

    fn accept_peer_data(&mut self, peer: ReceivedSegment) -> Result<(), FrameError> {
        if peer.sequence != self.peer_sequence {
            return Err(FrameError);
        }
        if peer.data_len > self.receive.len().saturating_sub(self.receive_len) {
            return Err(FrameError);
        }
        let end = self.receive_start + self.receive_len + peer.data_len;
        // SAFETY: `peer` identifies the serialized segment data produced by
        // the immediately preceding `receive_segment` call.
        let data = unsafe { &*SECURE_SEGMENT_DATA.0.get() };
        self.receive[self.receive_start + self.receive_len..end]
            .copy_from_slice(&data[..peer.data_len]);
        self.receive_len += peer.data_len;
        self.peer_sequence = self
            .peer_sequence
            .checked_add(peer.data_len as u32)
            .ok_or(FrameError)?;
        Ok(())
    }

    fn wait_for_ack(&mut self, acknowledgment: u32) -> Result<(), FrameError> {
        for _ in 0..MAX_RECEIVE_POLL_ATTEMPTS {
            let peer = self.receive_segment()?;
            if peer.data_len != 0 {
                self.accept_peer_data(peer)?;
                self.send_ack()?;
            }
            if peer.flags & pythos_user_tcp_probe::tcp::TCP_FLAG_FIN != 0 {
                self.peer_fin = true;
                self.peer_sequence = self.peer_sequence.checked_add(1).ok_or(FrameError)?;
                self.send_ack()?;
            }
            if peer.acknowledgment >= acknowledgment {
                return Ok(());
            }
        }
        Err(FrameError)
    }
}

impl ErrorType for TcpStream {
    type Error = FrameError;
}

impl Read for TcpStream {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, Self::Error> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.receive_len != 0 {
                let count = core::cmp::min(output.len(), self.receive_len);
                output[..count]
                    .copy_from_slice(&self.receive[self.receive_start..self.receive_start + count]);
                self.receive_start += count;
                self.receive_len -= count;
                if self.receive_len == 0 {
                    self.receive_start = 0;
                }
                return Ok(count);
            }
            if self.peer_fin {
                return Ok(0);
            }
            let peer = self.receive_segment()?;
            if peer.data_len != 0 {
                self.accept_peer_data(peer)?;
                self.send_ack()?;
                continue;
            }
            if peer.flags & pythos_user_tcp_probe::tcp::TCP_FLAG_FIN != 0 {
                if peer.sequence != self.peer_sequence {
                    return Err(FrameError);
                }
                self.peer_sequence = self.peer_sequence.checked_add(1).ok_or(FrameError)?;
                self.peer_fin = true;
                self.send_ack()?;
                return Ok(0);
            }
        }
    }
}

impl Write for TcpStream {
    fn write(&mut self, input: &[u8]) -> Result<usize, Self::Error> {
        let mut offset = 0;
        while offset < input.len() {
            let count = core::cmp::min(STREAM_TCP_PAYLOAD_BYTES, input.len() - offset);
            let sequence = self.local_sequence;
            self.send_segment(
                TCP_FLAG_ACK,
                sequence,
                self.peer_sequence,
                None,
                &input[offset..offset + count],
            )?;
            self.local_sequence = self
                .local_sequence
                .checked_add(count as u32)
                .ok_or(FrameError)?;
            self.wait_for_ack(self.local_sequence)?;
            offset += count;
        }
        Ok(input.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{SECURE_REQUEST, SECURE_RESPONSE, SERVER_CERT_DER};

    #[test]
    fn finite_secure_profile_is_bounded_and_pinned() {
        assert_eq!(SERVER_CERT_DER.len(), 311);
        assert_eq!(SECURE_REQUEST, b"PYTHOS-SECURE-REQUEST");
        assert_eq!(SECURE_RESPONSE, b"PYTHOS-SECURE-RESPONSE");
    }
}
