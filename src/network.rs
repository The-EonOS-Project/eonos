use alloc::vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const BROADCAST_IP: [u8; 4] = [255; 4];
const DHCP_PORT: u16 = 68;
const SERVER_PORT: u16 = 67;
const TCP_IDLE: u8 = 0;
const TCP_SYN_SENT: u8 = 1;
const TCP_ESTABLISHED: u8 = 2;
const TCP_CLOSING: u8 = 3;
const TCP_FAILED: u8 = 4;
const TCP_RX_CAPACITY: usize = 8192;
const TCP_MSS: usize = 536;
const TCP_FIN: u8 = 0x01;
const TCP_SYN: u8 = 0x02;
const TCP_RST: u8 = 0x04;
const TCP_PSH: u8 = 0x08;
const TCP_ACK: u8 = 0x10;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Config {
    pub address: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub server: [u8; 4],
    pub dns_server: [u8; 4],
    pub lease_seconds: u32,
    pub configured: u32,
}

static CONFIG: Mutex<Config> = Mutex::new(Config {
    address: [0; 4], netmask: [0; 4], gateway: [0; 4], server: [0; 4], dns_server: [0; 4],
    lease_seconds: 0, configured: 0,
});
static NEXT_XID: AtomicU32 = AtomicU32::new(0x454f_4e4f);
static ARP_CACHE: Mutex<Option<([u8; 4], [u8; 6])>> = Mutex::new(None);
#[derive(Clone, Copy)]
struct DnsEntry { name: [u8; 253], len: usize, address: Option<[u8; 4]>, expires_ms: u64 }
static DNS_CACHE: Mutex<[Option<DnsEntry>; 8]> = Mutex::new([None; 8]);
static NEXT_TCP_PORT: AtomicU32 = AtomicU32::new(49152);

struct TcpConnection {
    state: u8,
    local_port: u16,
    remote_port: u16,
    remote_ip: [u8; 4],
    remote_mac: [u8; 6],
    send_unacked: u32,
    send_next: u32,
    receive_next: u32,
    receive: [u8; TCP_RX_CAPACITY],
    receive_head: usize,
    receive_len: usize,
    peer_closed: bool,
    local_fin_acked: bool,
}

impl TcpConnection {
    const fn new() -> Self {
        Self { state: TCP_IDLE, local_port: 0, remote_port: 0, remote_ip: [0; 4],
            remote_mac: [0; 6], send_unacked: 0, send_next: 0, receive_next: 0,
            receive: [0; TCP_RX_CAPACITY], receive_head: 0, receive_len: 0,
            peer_closed: false, local_fin_acked: false }
    }
    fn reset(&mut self) { *self = Self::new(); }
    fn window(&self) -> u16 { (TCP_RX_CAPACITY - self.receive_len).min(u16::MAX as usize) as u16 }
}

static TCP_CONNECTION: Mutex<TcpConnection> = Mutex::new(TcpConnection::new());

pub fn config() -> Config { *CONFIG.lock() }

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    for pair in bytes.chunks_exact(2) { sum += u16::from_be_bytes([pair[0], pair[1]]) as u32; }
    if bytes.len() & 1 != 0 { sum += (bytes[bytes.len() - 1] as u32) << 8; }
    while sum >> 16 != 0 { sum = (sum & 0xffff) + (sum >> 16); }
    !(sum as u16)
}

fn ethernet(payload_type: u16, destination: [u8; 6], payload: &[u8]) -> Result<(), crate::drivers::e1000::Error> {
    let info = crate::drivers::e1000::info()?;

    let mut frame = vec![0u8; (14 + payload.len()).max(60)];
    frame[..6].copy_from_slice(&destination);
    frame[6..12].copy_from_slice(&info.mac);
    frame[12..14].copy_from_slice(&payload_type.to_be_bytes());
    frame[14..14 + payload.len()].copy_from_slice(payload);
    crate::drivers::e1000::transmit(&frame)
}

fn send_arp_reply(frame: &[u8]) {
    if frame.len() < 42 || frame[12..14] != [8, 6]
        || frame[14..16] != [0, 1] || frame[16..18] != [8, 0]
        || frame[18] != 6 || frame[19] != 4 || frame[20..22] != [0, 1] { return; }
    let target_ip: [u8; 4] = frame[38..42].try_into().unwrap();
    if target_ip != config().address { return; }
    let Some(info) = crate::drivers::e1000::info().ok() else { return; };
    let mut arp = [0u8; 28];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes()); arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes());
    arp[4] = 6; arp[5] = 4; arp[6..8].copy_from_slice(&2u16.to_be_bytes());
    arp[8..14].copy_from_slice(&info.mac); arp[14..18].copy_from_slice(&target_ip);
    arp[18..24].copy_from_slice(&frame[22..28]); arp[24..28].copy_from_slice(&frame[28..32]);
    let _ = ethernet(0x0806, frame[22..28].try_into().unwrap(), &arp);
}

fn send_udp(source_ip: [u8; 4], destination_ip: [u8; 4], destination_mac: [u8; 6], source_port: u16, destination_port: u16, payload: &[u8]) -> Result<(), crate::drivers::e1000::Error> {
    if payload.len() > 1472 { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    let mut packet = vec![0u8; 28 + payload.len()];
    packet[0] = 0x45; packet[2..4].copy_from_slice(&((20 + 8 + payload.len()) as u16).to_be_bytes());
    packet[4..6].copy_from_slice(&NEXT_XID.fetch_add(1, Ordering::Relaxed).to_be_bytes()[2..]);
    packet[8] = 64; packet[9] = 17; packet[12..16].copy_from_slice(&source_ip); packet[16..20].copy_from_slice(&destination_ip);
    let ip_sum = checksum(&packet[..20]); packet[10..12].copy_from_slice(&ip_sum.to_be_bytes());
    packet[20..22].copy_from_slice(&source_port.to_be_bytes()); packet[22..24].copy_from_slice(&destination_port.to_be_bytes());
    packet[24..26].copy_from_slice(&((8 + payload.len()) as u16).to_be_bytes()); packet[28..].copy_from_slice(payload);

    ethernet(0x0800, destination_mac, &packet)
}

fn resolve_mac(address: [u8; 4]) -> Result<[u8; 6], crate::drivers::e1000::Error> {
    if address == BROADCAST_IP { return Ok(BROADCAST_MAC); }
    if let Some((cached_ip, mac)) = *ARP_CACHE.lock() {
        if cached_ip == address { return Ok(mac); }
    }
    let info = crate::drivers::e1000::info()?;
    let mut arp = [0u8; 28];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes());
    arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes());
    arp[4] = 6; arp[5] = 4; arp[6..8].copy_from_slice(&1u16.to_be_bytes());
    arp[8..14].copy_from_slice(&info.mac);
    arp[14..18].copy_from_slice(&config().address);
    arp[24..28].copy_from_slice(&address);
    ethernet(0x0806, BROADCAST_MAC, &arp)?;
    let start = crate::pit::uptime_ms();
    let mut frame = [0u8; 1518];
    let mut received_frames = 0usize;
    loop {
        if crate::pit::uptime_ms().saturating_sub(start) > 1000 {
            crate::log_fail!("ARP", "Resolve", "No reply for {}.{}.{}.{}, received {} frame(s)", address[0], address[1], address[2], address[3], received_frames);
            return Err(crate::drivers::e1000::Error::ArpTimeout);
        }
        if let Some(length) = crate::drivers::e1000::receive(&mut frame)? {
            received_frames += 1;
            if length >= 42 && frame[12..14] == [8, 6]
                && frame[14..16] == [0, 1] && frame[16..18] == [8, 0]
                && frame[18] == 6 && frame[19] == 4
                && frame[20..22] == [0, 2] && frame[28..32] == address {
                let mac: [u8; 6] = frame[22..28].try_into().unwrap();
                *ARP_CACHE.lock() = Some((address, mac));
                return Ok(mac);
            }
            if frame[12..14] == [8, 6] { send_arp_reply(&frame[..length]); }
        } else { crate::scheduler::yield_now(); }
    }
}

pub fn udp_send(destination: [u8; 4], destination_port: u16, source_port: u16, payload: &[u8]) -> Result<usize, crate::drivers::e1000::Error> {
    let cfg = config();
    if cfg.configured == 0 { return Err(crate::drivers::e1000::Error::NoDevice); }
    if destination_port == 0 || source_port == 0 || payload.len() > 1472 { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    let same_subnet = (0..4).all(|i| (cfg.address[i] & cfg.netmask[i]) == (destination[i] & cfg.netmask[i]));
    let next_hop = if same_subnet { destination } else { cfg.gateway };
    let mac = resolve_mac(next_hop)?;
    send_udp(cfg.address, destination, mac, source_port, destination_port, payload)?;
    Ok(payload.len())
}

pub fn udp_receive(output: &mut [u8]) -> Result<Option<([u8; 4], u16, usize)>, crate::drivers::e1000::Error> {
    let mut frame = [0u8; 1518];
    let Some(length) = crate::drivers::e1000::receive(&mut frame)? else { return Ok(None); };
    if length >= 14 && frame[12..14] == [8, 6] { send_arp_reply(&frame[..length]); return Ok(None); }
    if length < 42 || frame[12..14] != [8, 0] || frame[23] != 17 { return Ok(None); }
    let ihl = (frame[14] as usize & 15) * 4;
    if ihl < 20 || length < 14 + ihl + 8 { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    let ip_total = u16::from_be_bytes([frame[16], frame[17]]) as usize;
    if ip_total < ihl + 8 || 14 + ip_total > length || checksum(&frame[14..14 + ihl]) != 0 { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    if u16::from_be_bytes([frame[20], frame[21]]) & 0x3fff != 0 { return Ok(None); }
    let cfg = config();
    let destination_ip: [u8; 4] = frame[30..34].try_into().unwrap();
    if destination_ip != cfg.address && destination_ip != BROADCAST_IP { return Ok(None); }
    let udp = 14 + ihl;
    let udp_len = u16::from_be_bytes([frame[udp + 4], frame[udp + 5]]) as usize;
    if udp_len < 8 || udp_len > ip_total - ihl { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    let payload_len = udp_len - 8;
    if payload_len > output.len() { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    output[..payload_len].copy_from_slice(&frame[udp + 8..udp + 8 + payload_len]);
    let source_ip = frame[26..30].try_into().unwrap();
    let source_port = u16::from_be_bytes([frame[udp], frame[udp + 1]]);
    Ok(Some((source_ip, source_port, payload_len)))
}

fn tcp_packet(destination_mac: [u8; 6], destination_ip: [u8; 4], source_port: u16,
    destination_port: u16, sequence: u32, acknowledgement: u32, flags: u8,
    window: u16, payload: &[u8]) -> Result<(), crate::drivers::e1000::Error> {
    let source_ip = config().address;
    let option_len = if flags & TCP_SYN != 0 { 4 } else { 0 };
    let header_len = 20 + option_len;
    let tcp_len = header_len + payload.len();
    if tcp_len > u16::MAX as usize || tcp_len + 20 > 1500 {
        return Err(crate::drivers::e1000::Error::InvalidFrame);
    }
    let mut tcp = vec![0u8; tcp_len];
    tcp[0..2].copy_from_slice(&source_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&destination_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&sequence.to_be_bytes());
    tcp[8..12].copy_from_slice(&acknowledgement.to_be_bytes());
    tcp[12] = ((header_len / 4) as u8) << 4;
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&window.to_be_bytes());
    if option_len != 0 {
        tcp[20..24].copy_from_slice(&[2, 4, (TCP_MSS >> 8) as u8, TCP_MSS as u8]);
    }
    tcp[header_len..].copy_from_slice(payload);
    let mut pseudo = vec![0u8; 12 + tcp_len];
    pseudo[0..4].copy_from_slice(&source_ip);
    pseudo[4..8].copy_from_slice(&destination_ip);
    pseudo[9] = 6;
    pseudo[10..12].copy_from_slice(&(tcp_len as u16).to_be_bytes());
    pseudo[12..].copy_from_slice(&tcp);
    let tcp_sum = checksum(&pseudo);
    tcp[16..18].copy_from_slice(&tcp_sum.to_be_bytes());

    let mut packet = vec![0u8; 20 + tcp_len];
    packet[0] = 0x45;
    let packet_len = packet.len() as u16;
    packet[2..4].copy_from_slice(&packet_len.to_be_bytes());
    packet[4..6].copy_from_slice(&NEXT_XID.fetch_add(1, Ordering::Relaxed).to_be_bytes()[2..]);
    packet[8] = 64;
    packet[9] = 6;
    packet[12..16].copy_from_slice(&source_ip);
    packet[16..20].copy_from_slice(&destination_ip);
    packet[20..].copy_from_slice(&tcp);
    let ip_sum = checksum(&packet[..20]);
    packet[10..12].copy_from_slice(&ip_sum.to_be_bytes());
    ethernet(0x0800, destination_mac, &packet)
}

fn tcp_emit_current(flags: u8, sequence: u32, acknowledgement: u32, payload: &[u8])
    -> Result<(), crate::drivers::e1000::Error> {
    let (mac, ip, local_port, remote_port, window) = {
        let connection = TCP_CONNECTION.lock();
        if connection.state == TCP_IDLE || connection.state == TCP_FAILED {
            return Err(crate::drivers::e1000::Error::TcpNotConnected);
        }
        (connection.remote_mac, connection.remote_ip, connection.local_port,
            connection.remote_port, connection.window())
    };
    tcp_packet(mac, ip, local_port, remote_port, sequence, acknowledgement, flags, window, payload)
}

fn tcp_poll_frame() -> Result<bool, crate::drivers::e1000::Error> {
    let mut frame = [0u8; 1518];
    let Some(length) = crate::drivers::e1000::receive(&mut frame)? else { return Ok(false); };
    if length >= 42 && frame[12..14] == [8, 6] { send_arp_reply(&frame[..length]); return Ok(true); }
    if length < 54 || frame[12..14] != [8, 0] || frame[23] != 6 { return Ok(true); }
    let ip = 14;
    let ihl = (frame[ip] as usize & 15) * 4;
    let total = u16::from_be_bytes([frame[ip + 2], frame[ip + 3]]) as usize;
    if ihl < 20 || total < ihl + 20 || ip + total > length
        || checksum(&frame[ip..ip + ihl]) != 0
        || u16::from_be_bytes([frame[ip + 6], frame[ip + 7]]) & 0x3fff != 0 {
        return Ok(true);
    }
    let source_ip: [u8; 4] = frame[ip + 12..ip + 16].try_into().unwrap();
    let destination_ip: [u8; 4] = frame[ip + 16..ip + 20].try_into().unwrap();
    let cfg = config();
    if destination_ip != cfg.address { return Ok(true); }
    let tcp = ip + ihl;
    let tcp_len = total - ihl;
    let header_len = (frame[tcp + 12] as usize >> 4) * 4;
    if header_len < 20 || header_len > tcp_len { return Ok(true); }
    let mut pseudo = vec![0u8; 12 + tcp_len];
    pseudo[0..4].copy_from_slice(&source_ip);
    pseudo[4..8].copy_from_slice(&destination_ip);
    pseudo[9] = 6;
    pseudo[10..12].copy_from_slice(&(tcp_len as u16).to_be_bytes());
    pseudo[12..].copy_from_slice(&frame[tcp..tcp + tcp_len]);
    if checksum(&pseudo) != 0 { return Ok(true); }

    let source_port = u16::from_be_bytes([frame[tcp], frame[tcp + 1]]);
    let destination_port = u16::from_be_bytes([frame[tcp + 2], frame[tcp + 3]]);
    let sequence = u32::from_be_bytes(frame[tcp + 4..tcp + 8].try_into().unwrap());
    let acknowledgement = u32::from_be_bytes(frame[tcp + 8..tcp + 12].try_into().unwrap());
    let flags = frame[tcp + 13];
    let payload = &frame[tcp + header_len..tcp + tcp_len];
    let mut send_ack = false;
    let mut ack_sequence = 0;
    let mut ack_number = 0;
    let source_mac: [u8; 6] = frame[6..12].try_into().unwrap();
    {
        let mut connection = TCP_CONNECTION.lock();
        if destination_port != connection.local_port || source_port != connection.remote_port
            || source_ip != connection.remote_ip || connection.state == TCP_IDLE {
            return Ok(true);
        }
        if flags & TCP_RST != 0 {
            crate::log_fail!("TCP", "Input", "RST from {}.{}.{}.{}:{} while connection state {}",
                source_ip[0], source_ip[1], source_ip[2], source_ip[3], source_port, connection.state);
            connection.state = TCP_FAILED;
            return Ok(true);
        }
        if connection.state == TCP_SYN_SENT {
            if flags & (TCP_SYN | TCP_ACK) == (TCP_SYN | TCP_ACK)
                && acknowledgement == connection.send_next {
                connection.receive_next = sequence.wrapping_add(1);
                connection.send_unacked = acknowledgement;
                connection.state = TCP_ESTABLISHED;
                send_ack = true;
            }
        } else if connection.state == TCP_ESTABLISHED || connection.state == TCP_CLOSING {
            if flags & TCP_ACK != 0
                && acknowledgement.wrapping_sub(connection.send_unacked)
                    <= connection.send_next.wrapping_sub(connection.send_unacked) {
                connection.send_unacked = acknowledgement;
                if connection.state == TCP_CLOSING && acknowledgement == connection.send_next {
                    connection.local_fin_acked = true;
                }
            }
            let mut expected = connection.receive_next;
            if !payload.is_empty() {
                if sequence == expected {
                    let available = TCP_RX_CAPACITY - connection.receive_len;
                    if payload.len() <= available {
                        for byte in payload {
                            let tail = (connection.receive_head + connection.receive_len) % TCP_RX_CAPACITY;
                            connection.receive[tail] = *byte;
                            connection.receive_len += 1;
                        }
                        connection.receive_next = connection.receive_next.wrapping_add(payload.len() as u32);
                        expected = connection.receive_next;
                    }
                }
                send_ack = true;
            }
            if flags & TCP_FIN != 0 {
                if sequence.wrapping_add(payload.len() as u32) == expected {
                    connection.receive_next = connection.receive_next.wrapping_add(1);
                    connection.peer_closed = true;
                }
                send_ack = true;
            }
        }
        if send_ack {
            connection.remote_mac = source_mac;
            ack_sequence = connection.send_next;
            ack_number = connection.receive_next;
        }
    }
    if send_ack {
        tcp_emit_current(TCP_ACK, ack_sequence, ack_number, &[])?;
    }
    Ok(true)
}

pub fn tcp_connect(destination: [u8; 4], destination_port: u16) -> Result<(), crate::drivers::e1000::Error> {
    let cfg = config();
    if cfg.configured == 0 { return Err(crate::drivers::e1000::Error::NoDevice); }
    if destination_port == 0 || destination == [0; 4] || destination == BROADCAST_IP {
        return Err(crate::drivers::e1000::Error::InvalidFrame);
    }
    {
        let connection = TCP_CONNECTION.lock();
        if connection.state != TCP_IDLE { return Err(crate::drivers::e1000::Error::Busy); }
    }
    let same_subnet = (0..4).all(|i| (cfg.address[i] & cfg.netmask[i]) == (destination[i] & cfg.netmask[i]));
    let next_hop = if same_subnet { destination } else { cfg.gateway };
    let mac = resolve_mac(next_hop)?;
    let local_port = (NEXT_TCP_PORT.fetch_add(1, Ordering::Relaxed) as u16).max(49152);
    let initial = NEXT_XID.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9e37_79b9);
    {
        let mut connection = TCP_CONNECTION.lock();
        *connection = TcpConnection::new();
        connection.state = TCP_SYN_SENT;
        connection.local_port = local_port;
        connection.remote_port = destination_port;
        connection.remote_ip = destination;
        connection.remote_mac = mac;
        connection.send_unacked = initial;
        connection.send_next = initial.wrapping_add(1);
    }
    let mut last_sent = crate::pit::uptime_ms();
    if let Err(error) = tcp_emit_current(TCP_SYN, initial, 0, &[]) {
        TCP_CONNECTION.lock().reset();
        return Err(error);
    }
    loop {
        let state = TCP_CONNECTION.lock().state;
        if state == TCP_ESTABLISHED { return Ok(()); }
        if state == TCP_FAILED { TCP_CONNECTION.lock().reset(); return Err(crate::drivers::e1000::Error::TcpReset); }
        let now = crate::pit::uptime_ms();
        if now.saturating_sub(last_sent) >= 1000 {
            if now.saturating_sub(last_sent) >= 5000 {
                TCP_CONNECTION.lock().reset();
                return Err(crate::drivers::e1000::Error::Timeout);
            }
            if let Err(error) = tcp_emit_current(TCP_SYN, initial, 0, &[]) {
                TCP_CONNECTION.lock().reset();
                return Err(error);
            }
            last_sent = now;
        }
        match tcp_poll_frame() {
            Ok(false) => crate::scheduler::yield_now(),
            Ok(true) => {}
            Err(error) => { TCP_CONNECTION.lock().reset(); return Err(error); }
        }
    }
}

pub fn tcp_send(mut bytes: &[u8]) -> Result<usize, crate::drivers::e1000::Error> {
    if TCP_CONNECTION.lock().state != TCP_ESTABLISHED {
        return Err(crate::drivers::e1000::Error::TcpNotConnected);
    }
    let total = bytes.len();
    while !bytes.is_empty() {
        let chunk_len = bytes.len().min(TCP_MSS);
        let chunk = &bytes[..chunk_len];
        let (sequence, acknowledgement) = {
            let mut connection = TCP_CONNECTION.lock();
            if connection.state != TCP_ESTABLISHED { return Err(crate::drivers::e1000::Error::TcpReset); }
            let sequence = connection.send_next;
            connection.send_next = connection.send_next.wrapping_add(chunk_len as u32);
            (sequence, connection.receive_next)
        };
        let mut acknowledged = false;
        for _ in 0..5 {
            tcp_emit_current(TCP_ACK | TCP_PSH, sequence, acknowledgement, chunk)?;
            let started = crate::pit::uptime_ms();
            loop {
                {
                    let connection = TCP_CONNECTION.lock();
                    if connection.state == TCP_FAILED { return Err(crate::drivers::e1000::Error::TcpReset); }
                    if connection.send_unacked == sequence.wrapping_add(chunk_len as u32) {
                        acknowledged = true;
                        break;
                    }
                }
                if crate::pit::uptime_ms().saturating_sub(started) >= 1000 { break; }
                let _ = tcp_poll_frame()?;
                crate::scheduler::yield_now();
            }
            if acknowledged { break; }
        }
        if !acknowledged { return Err(crate::drivers::e1000::Error::Timeout); }
        bytes = &bytes[chunk_len..];
    }
    Ok(total)
}

pub fn tcp_receive(output: &mut [u8], timeout_ms: u64) -> Result<usize, crate::drivers::e1000::Error> {
    if output.is_empty() { return Ok(0); }
    let started = crate::pit::uptime_ms();
    loop {
        {
            let mut connection = TCP_CONNECTION.lock();
            if connection.state == TCP_FAILED { return Err(crate::drivers::e1000::Error::TcpReset); }
            if connection.state == TCP_IDLE { return Err(crate::drivers::e1000::Error::TcpNotConnected); }
            if connection.receive_len != 0 {
                let count = output.len().min(connection.receive_len);
                for slot in &mut output[..count] {
                    *slot = connection.receive[connection.receive_head];
                    connection.receive_head = (connection.receive_head + 1) % TCP_RX_CAPACITY;
                    connection.receive_len -= 1;
                }
                let sequence = connection.send_next;
                let acknowledgement = connection.receive_next;
                drop(connection);

                tcp_emit_current(TCP_ACK, sequence, acknowledgement, &[])?;
                return Ok(count);
            }
            if connection.peer_closed { return Ok(0); }
        }
        if crate::pit::uptime_ms().saturating_sub(started) >= timeout_ms {
            return Err(crate::drivers::e1000::Error::Timeout);
        }
        if !tcp_poll_frame()? { crate::scheduler::yield_now(); }
    }
}

pub fn tcp_close() -> Result<(), crate::drivers::e1000::Error> {
    let (sequence, acknowledgement) = {
        let mut connection = TCP_CONNECTION.lock();
        if connection.state == TCP_IDLE { return Ok(()); }
        if connection.state == TCP_FAILED { connection.reset(); return Ok(()); }
        let sequence = connection.send_next;
        connection.send_next = connection.send_next.wrapping_add(1);
        connection.state = TCP_CLOSING;
        (sequence, connection.receive_next)
    };
    let _ = tcp_emit_current(TCP_FIN | TCP_ACK, sequence, acknowledgement, &[]);
    let started = crate::pit::uptime_ms();
    while crate::pit::uptime_ms().saturating_sub(started) < 1000 {
        let connection = TCP_CONNECTION.lock();
        if connection.local_fin_acked && connection.peer_closed { break; }
        drop(connection);
        if !tcp_poll_frame()? { crate::scheduler::yield_now(); }
    }
    TCP_CONNECTION.lock().reset();
    Ok(())
}

fn skip_dns_name(packet: &[u8], offset: &mut usize) -> Option<()> {
    loop {
        let length = *packet.get(*offset)?;
        if length == 0 { *offset += 1; return Some(()); }
        if length & 0xc0 == 0xc0 {
            packet.get(*offset + 1)?;
            *offset += 2;
            return Some(());
        }
        if length & 0xc0 != 0 || length > 63 { return None; }
        *offset = offset.checked_add(1 + length as usize)?;
        packet.get(*offset - 1)?;
    }
}

fn parse_dns_a(packet: &[u8], id: u16) -> Option<(Option<[u8; 4]>, u32)> {
    if packet.len() < 12 || u16::from_be_bytes([packet[0], packet[1]]) != id { return None; }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if flags & 0x8000 == 0 || flags & 0x0200 != 0 { return None; }
    match flags & 0x000f {
        0 => {}
        3 => return Some((None, 30)), 
        _ => return None,
    }
    let questions = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let answers = u16::from_be_bytes([packet[6], packet[7]]) as usize;
    let mut offset = 12;
    for _ in 0..questions {
        skip_dns_name(packet, &mut offset)?;
        offset = offset.checked_add(4)?;
        if offset > packet.len() { return None; }
    }
    let mut found = None;
    let mut ttl = u32::MAX;
    for _ in 0..answers {
        skip_dns_name(packet, &mut offset)?;
        let fixed = packet.get(offset..offset.checked_add(10)?)?;
        let kind = u16::from_be_bytes([fixed[0], fixed[1]]);
        let class = u16::from_be_bytes([fixed[2], fixed[3]]);
        let length = u16::from_be_bytes([fixed[8], fixed[9]]) as usize;
        offset += 10;
        let data = packet.get(offset..offset.checked_add(length)?)?;
        if kind == 1 && class == 1 && length == 4 {
            if found.is_none() {
                found = Some(data.try_into().ok()?);
                ttl = u32::from_be_bytes(fixed[4..8].try_into().ok()?);
            }
        }
        offset += length;
    }
    Some((found, if found.is_some() { ttl } else { 30 }))
}

fn cached_dns(name: &str, now: u64) -> Option<Option<[u8; 4]>> {
    let cache = DNS_CACHE.lock();
    cache.iter().flatten().find(|entry| entry.expires_ms > now
        && entry.len == name.len()
        && entry.name[..entry.len].iter().zip(name.bytes()).all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase()))
        .map(|entry| entry.address)
}

fn cache_dns(name: &str, address: Option<[u8; 4]>, ttl: u32, now: u64) {
    let mut entry_name = [0u8; 253];
    entry_name[..name.len()].copy_from_slice(name.as_bytes());
    let entry = DnsEntry { name: entry_name, len: name.len(), address,
        expires_ms: now.saturating_add(ttl.clamp(1, 86400) as u64 * 1000) };
    let mut cache = DNS_CACHE.lock();
    let slot = cache.iter().position(|item| item.is_none_or(|old| old.len == name.len()
        && old.name[..old.len].iter().zip(name.bytes()).all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
        || old.expires_ms <= now)).unwrap_or(0);
    cache[slot] = Some(entry);
}

pub fn resolve_ipv4(hostname: &str) -> Result<Option<[u8; 4]>, crate::drivers::e1000::Error> {
    let cfg = config();
    if cfg.configured == 0 { return Err(crate::drivers::e1000::Error::NoDevice); }
    let name = hostname.strip_suffix('.').unwrap_or(hostname);
    if name.is_empty() || name.len() > 253 || cfg.dns_server == [0; 4] {
        return Err(crate::drivers::e1000::Error::InvalidFrame);
    }
    if !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.') {
        return Err(crate::drivers::e1000::Error::InvalidFrame);
    }
    if let Some(address) = cached_dns(name, crate::pit::uptime_ms()) { return Ok(address); }
    let mut query = vec![0u8; 12];
    let id = NEXT_XID.fetch_add(1, Ordering::Relaxed) as u16;
    query[0..2].copy_from_slice(&id.to_be_bytes());
    query[2..4].copy_from_slice(&0x0100u16.to_be_bytes()); 
    query[4..6].copy_from_slice(&1u16.to_be_bytes());
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
            || !label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            return Err(crate::drivers::e1000::Error::InvalidFrame);
        }
        query.push(label.len() as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.extend_from_slice(&[0, 0, 1, 0, 1]); 
    if query.len() > 512 { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    udp_send(cfg.dns_server, 53, 53000, &query)?;

    let started = crate::pit::uptime_ms();
    let mut response = [0u8; 1472];
    loop {
        if crate::pit::uptime_ms().saturating_sub(started) > 3000 {
            return Err(crate::drivers::e1000::Error::Timeout);
        }
        if let Some((source, port, length)) = udp_receive(&mut response)? {
            if source == cfg.dns_server && port == 53 {
                if let Some((answer, ttl)) = parse_dns_a(&response[..length], id) {
                    cache_dns(name, answer, ttl, crate::pit::uptime_ms());
                    return Ok(answer);
                }
            }
        } else {
            crate::scheduler::yield_now();
        }
    }
}

pub fn ping(destination: [u8; 4], timeout_ms: u64) -> Result<Option<u64>, crate::drivers::e1000::Error> {
    let cfg = config();
    if cfg.configured == 0 { return Err(crate::drivers::e1000::Error::NoDevice); }
    if destination == [0; 4] || destination == BROADCAST_IP { return Err(crate::drivers::e1000::Error::InvalidFrame); }
    let same_subnet = (0..4).all(|i| (cfg.address[i] & cfg.netmask[i]) == (destination[i] & cfg.netmask[i]));
    let next_hop = if same_subnet { destination } else { cfg.gateway };
    let destination_mac = resolve_mac(next_hop)?;
    let sequence = NEXT_XID.fetch_add(1, Ordering::Relaxed) as u16;
    let identifier = 0xe050u16;
    let mut icmp = [0u8; 16];
    icmp[0] = 8; 
    icmp[4..6].copy_from_slice(&identifier.to_be_bytes());
    icmp[6..8].copy_from_slice(&sequence.to_be_bytes());
    icmp[8..].copy_from_slice(&crate::pit::uptime_ms().to_be_bytes());
    let icmp_sum = checksum(&icmp);
    icmp[2..4].copy_from_slice(&icmp_sum.to_be_bytes());
    let mut packet = vec![0u8; 20 + icmp.len()];
    packet[0] = 0x45;
    let packet_len = packet.len() as u16;
    packet[2..4].copy_from_slice(&packet_len.to_be_bytes());
    packet[4..6].copy_from_slice(&sequence.to_be_bytes());
    packet[8] = 64;
    packet[9] = 1;
    packet[12..16].copy_from_slice(&cfg.address);
    packet[16..20].copy_from_slice(&destination);
    let ip_sum = checksum(&packet[..20]);
    packet[10..12].copy_from_slice(&ip_sum.to_be_bytes());
    packet[20..].copy_from_slice(&icmp);
    ethernet(0x0800, destination_mac, &packet)?;

    let started = crate::pit::uptime_ms();
    let mut frame = [0u8; 1518];
    loop {
        let elapsed = crate::pit::uptime_ms().saturating_sub(started);
        if elapsed >= timeout_ms { return Ok(None); }
        if let Some(length) = crate::drivers::e1000::receive(&mut frame)? {
            if length >= 42 && frame[12..14] == [8, 6] { send_arp_reply(&frame[..length]); continue; }
            if length < 42 || frame[12..14] != [8, 0] || frame[23] != 1 { continue; }
            let ihl = (frame[14] as usize & 15) * 4;
            let total = u16::from_be_bytes([frame[16], frame[17]]) as usize;
            if ihl < 20 || total < ihl + 8 || 14 + total > length || checksum(&frame[14..14 + ihl]) != 0 { continue; }
            if u16::from_be_bytes([frame[20], frame[21]]) & 0x3fff != 0 { continue; }
            let icmp = 14 + ihl;
            if frame[icmp] != 0 || frame[icmp + 1] != 0 || checksum(&frame[icmp..14 + total]) != 0 { continue; }
            if frame[icmp + 4..icmp + 6] == identifier.to_be_bytes()
                && frame[icmp + 6..icmp + 8] == sequence.to_be_bytes()
                && frame[26..30] == destination {
                return Ok(Some(crate::pit::uptime_ms().saturating_sub(started)));
            }
        } else {
            crate::scheduler::yield_now();
        }
    }
}

fn dhcp_message(info: &crate::drivers::e1000::NetworkInfo, xid: u32, message: u8, requested: Option<([u8; 4], [u8; 4])>) -> [u8; 300] {
    let mut p = [0u8; 300];
    p[0] = 1; p[1] = 1; p[2] = 6; p[4..8].copy_from_slice(&xid.to_be_bytes());
    p[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); p[28..34].copy_from_slice(&info.mac);
    p[236..240].copy_from_slice(&[99, 130, 83, 99]);
    let mut i = 240; p[i..i+3].copy_from_slice(&[53, 1, message]); i += 3;
    p[i..i+2].copy_from_slice(&[55, 4]); p[i+2..i+6].copy_from_slice(&[1, 3, 6, 51]); i += 6;
    if let Some((address, server)) = requested {
        p[i..i+2].copy_from_slice(&[50, 4]); p[i+2..i+6].copy_from_slice(&address); i += 6;
        p[i..i+2].copy_from_slice(&[54, 4]); p[i+2..i+6].copy_from_slice(&server); i += 6;
    }
    p[i] = 255; p
}

fn parse_dhcp(payload: &[u8], xid: u32, mac: &[u8; 6]) -> Option<(u8, [u8; 4], Config)> {
    if payload.len() < 240 || payload[0] != 2 || payload[4..8] != xid.to_be_bytes() || payload[28..34] != mac[..] || payload[236..240] != [99,130,83,99] { return None; }
    let offered: [u8; 4] = payload[16..20].try_into().ok()?;
    let mut cfg = Config { address: offered, ..Config::default() };
    let mut message = 0;
    let mut i = 240;
    while i < payload.len() {
        let kind = payload[i]; i += 1;
        if kind == 255 { break; }
        if kind == 0 { continue; }
        let len = *payload.get(i)? as usize; i += 1;
        let data = payload.get(i..i + len)?; i += len;
        match (kind, len) {
            (53, 1) => message = data[0], (1, 4) => cfg.netmask.copy_from_slice(data),
            (3, n) if n >= 4 => cfg.gateway.copy_from_slice(&data[..4]),
            (54, 4) => cfg.server.copy_from_slice(data),
            (6, n) if n >= 4 => cfg.dns_server.copy_from_slice(&data[..4]),
            (51, 4) => cfg.lease_seconds = u32::from_be_bytes(data.try_into().ok()?),
            _ => {}
        }
    }
    Some((message, offered, cfg))
}

pub fn dhcp() -> Result<Config, crate::drivers::e1000::Error> {
    let info = crate::drivers::e1000::info()?;
    let xid = NEXT_XID.fetch_add(1, Ordering::Relaxed);
    let discover = dhcp_message(&info, xid, 1, None);
    send_udp([0; 4], BROADCAST_IP, BROADCAST_MAC, DHCP_PORT, SERVER_PORT, &discover)?;
    let start = crate::pit::uptime_ms();
    let mut frame = [0u8; 1518];
    let (offered, server) = loop {
        if crate::pit::uptime_ms().saturating_sub(start) > 5000 { return Err(crate::drivers::e1000::Error::Busy); }
        if let Some(length) = crate::drivers::e1000::receive(&mut frame)? {
            if length >= 14 && u16::from_be_bytes([frame[12], frame[13]]) == 0x0806 { send_arp_reply(&frame[..length]); continue; }
            if length < 42 || frame[12..14] != [8,0] || frame[23] != 17 { continue; }
            let ihl = (frame[14] as usize & 15) * 4;
            if ihl < 20 || length < 14 + ihl + 8 { continue; }
            let udp = 14 + ihl;
            let src_port = u16::from_be_bytes([frame[udp], frame[udp+1]]);
            let dst_port = u16::from_be_bytes([frame[udp+2], frame[udp+3]]);
            if src_port != SERVER_PORT || dst_port != DHCP_PORT { continue; }
            let payload_start = udp + 8;
            let udp_len = u16::from_be_bytes([frame[udp+4], frame[udp+5]]) as usize;
            if udp_len < 8 || payload_start + udp_len - 8 > length { continue; }
            if let Some((kind, address, cfg)) = parse_dhcp(&frame[payload_start..payload_start+udp_len-8], xid, &info.mac) {
                if kind == 2 { break (address, cfg.server); }
            }
        } else { crate::scheduler::yield_now(); }
    };
    let request = dhcp_message(&info, xid, 3, Some((offered, server)));
    send_udp([0; 4], BROADCAST_IP, BROADCAST_MAC, DHCP_PORT, SERVER_PORT, &request)?;
    let start = crate::pit::uptime_ms();
    loop {
        if crate::pit::uptime_ms().saturating_sub(start) > 5000 { return Err(crate::drivers::e1000::Error::Busy); }
        if let Some(length) = crate::drivers::e1000::receive(&mut frame)? {
            if length < 42 || frame[12..14] != [8,0] || frame[23] != 17 { continue; }
            let ihl = (frame[14] as usize & 15) * 4; let udp = 14 + ihl;
            if ihl < 20 || length < udp + 8 || u16::from_be_bytes([frame[udp],frame[udp+1]]) != SERVER_PORT || u16::from_be_bytes([frame[udp+2],frame[udp+3]]) != DHCP_PORT { continue; }
            let start_payload = udp + 8; let udp_len = u16::from_be_bytes([frame[udp+4],frame[udp+5]]) as usize;
            if udp_len < 8 || start_payload + udp_len - 8 > length { continue; }
            if let Some((kind, _, mut cfg)) = parse_dhcp(&frame[start_payload..start_payload+udp_len-8], xid, &info.mac) {
                if kind == 5 {
                    cfg.configured = 1;
                    let dns_changed = CONFIG.lock().dns_server != cfg.dns_server;
                    *CONFIG.lock() = cfg;
                    if dns_changed { *DNS_CACHE.lock() = [None; 8]; }
                    return Ok(cfg);
                }
            }
        } else { crate::scheduler::yield_now(); }
    }
}
