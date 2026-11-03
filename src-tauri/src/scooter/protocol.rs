use btleplug::api::bleuuid::uuid_from_u16;
use uuid::Uuid;

pub const HANDSHAKE_UUID: Uuid = Uuid::from_u128(0xbef8e7e0_9c21_4c9e_b632_bd58c1009f9f);
pub const HANDSHAKE_REPLY_UUID: Uuid = Uuid::from_u128(0xbef8e7e1_9c21_4c9e_b632_bd58c1009f9f);
pub const FRAME_UUID: Uuid = Uuid::from_u128(0xbef8e820_9c21_4c9e_b632_bd58c1009f9f);
pub const COMMAND_UUID: Uuid = Uuid::from_u128(0xbef8e821_9c21_4c9e_b632_bd58c1009f9f);
pub const SERIAL_UUID: Uuid = uuid_from_u16(0x2a25);

const HEADER: [u8; 2] = [0x59, 0x44];
const TAIL: [u8; 2] = [0x4b, 0x4a];
const FRAME_LEN: usize = 177;
const COMMAND_ID: u8 = 0x3e;
const PAIRING_ACCEPTED: [u8; 3] = [0x5a, 0x96, 0x00];
const SOC_BYTE: usize = 87;
const SPEED_BYTES: usize = 75;
const STATE_BYTES: std::ops::RangeInclusive<usize> = 4..=9;

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub soc1: Option<f64>,
    pub speed: Option<f64>,
    pub state_bytes: [u8; 6],
}

fn int8(byte: u8) -> i32 {
    byte as i8 as i32
}

fn three_bytes(value: i32) -> [u8; 3] {
    [(value >> 16) as u8, (value >> 8) as u8, value as u8]
}

/// The vendor app sign-extends the first two challenge bytes; the device
/// expects the same quirk.
pub fn pairing_packet(challenge: &[u8], hour: u32, minute: u32, second: u32, mac: [u8; 6]) -> [u8; 14] {
    let byte = |index: usize| challenge.get(index).copied().unwrap_or(0);
    let i = (int8(byte(0)).wrapping_shl(16))
        .wrapping_add(int8(byte(1)).wrapping_shl(8))
        .wrapping_add(byte(2) as i32);
    let i2 = ((hour << 16) | (minute << 8) | second) as i32;
    let i3 = i | i2;
    let v1 = ((mac[0] as i32) << 16 | (mac[1] as i32) << 8 | mac[2] as i32) ^ i3;
    let v2 = ((mac[3] as i32) << 16 | (mac[4] as i32) << 8 | mac[5] as i32) ^ i3;
    let mut packet = [0u8; 14];
    packet[0..3].copy_from_slice(&three_bytes(i3));
    packet[3..6].copy_from_slice(&three_bytes(v1));
    packet[6..9].copy_from_slice(&three_bytes(v2));
    packet[9..].copy_from_slice(&[0x41, 0x05, 0x01, 0xff, 0x45 ^ 0xff]);
    packet
}

pub fn power_packet(on: bool) -> Vec<u8> {
    let mut body = vec![0x01, 0x00, 0x01, 0x01, 0xf8, 0xff, 0xff, if on { 0x02 } else { 0x03 }];
    body.extend(std::iter::repeat_n(0xff, 22));
    let checksum = body
        .iter()
        .fold(COMMAND_ID as u32 + body.len() as u32, |sum, byte| sum + *byte as u32) as u8;
    let mut packet = vec![HEADER[0], HEADER[1], COMMAND_ID, body.len() as u8];
    packet.extend(body);
    packet.extend([checksum, TAIL[0], TAIL[1]]);
    packet
}

pub fn is_pairing_accepted(value: &[u8]) -> bool {
    value == PAIRING_ACCEPTED
}

pub fn parse_frame(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() != FRAME_LEN || bytes[..2] != HEADER || bytes[FRAME_LEN - 2..] != TAIL {
        return None;
    }
    let checksum = bytes[2..174].iter().fold(0u32, |sum, byte| sum + *byte as u32) as u8;
    if checksum != bytes[174] {
        return None;
    }
    let soc_raw = bytes[SOC_BYTE];
    let speed_raw = [bytes[SPEED_BYTES], bytes[SPEED_BYTES + 1]];
    let mut state_bytes = [0u8; 6];
    state_bytes.copy_from_slice(&bytes[STATE_BYTES]);
    Some(Frame {
        soc1: (soc_raw <= 200).then_some(soc_raw as f64 * 0.5),
        speed: (speed_raw != [0xff, 0xff]).then(|| u16::from_le_bytes(speed_raw) as f64 * 0.1),
        state_bytes,
    })
}

/// Notifications can split a status frame; this pulls whole frames out of
/// the bytes received so far and drops anything that can't start one.
pub fn drain_frames(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    loop {
        match buffer.windows(2).position(|pair| pair == HEADER) {
            Some(start) => {
                buffer.drain(..start);
            }
            None => {
                let keep = usize::from(buffer.last() == Some(&HEADER[0]));
                buffer.drain(..buffer.len() - keep);
                return frames;
            }
        }
        if buffer.len() < 4 {
            return frames;
        }
        let total = buffer[3] as usize + 7;
        if buffer.len() < total {
            return frames;
        }
        if buffer[total - 2..total] == TAIL {
            frames.push(buffer.drain(..total).collect());
        } else {
            buffer.drain(..2);
        }
    }
}

pub fn mac_from_serial(serial: &[u8]) -> Option<[u8; 6]> {
    let mut mac: [u8; 6] = serial.get(..6)?.try_into().ok()?;
    mac.reverse();
    Some(mac)
}

pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let digits: String = text.chars().filter(|c| !matches!(c, ':' | '-' | ' ')).collect();
    if digits.len() != 12 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (index, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(digits.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(mac)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0xc1, 0x3a, 0x64, 0x07, 0x15, 0x40];

    fn frame(soc: u8, speed: [u8; 2]) -> Vec<u8> {
        let mut bytes = vec![0xff; FRAME_LEN];
        bytes[..4].copy_from_slice(&[0x59, 0x44, 0x2a, 0xaa]);
        bytes[4..10].copy_from_slice(&[0x01, 0x00, 0x00, 0x00, 0x02, 0x00]);
        bytes[SOC_BYTE] = soc;
        bytes[SPEED_BYTES..SPEED_BYTES + 2].copy_from_slice(&speed);
        bytes[174] = bytes[2..174].iter().fold(0u32, |sum, byte| sum + *byte as u32) as u8;
        bytes[175..].copy_from_slice(&TAIL);
        bytes
    }

    #[test]
    fn pairing_packet_matches_the_spec_test_vector() {
        let packet = pairing_packet(&[0xb6, 0xc4, 0xa8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00, 0x01], 14, 5, 9, MAC);
        assert_eq!(hex(&packet), "BF C5 A9 7E FF CD B8 D0 E9 41 05 01 FF BA");
    }

    #[test]
    fn pairing_packet_keeps_positive_challenge_bytes_unsigned() {
        let packet = pairing_packet(&[0x0b, 0x1a, 0xb0], 0, 0, 0, MAC);
        assert_eq!(packet[..3], [0x0b, 0x1a, 0xb0]);
    }

    #[test]
    fn power_packets_match_the_spec() {
        let ff22 = vec!["FF"; 22].join(" ");
        assert_eq!(hex(&power_packet(true)), format!("59 44 3E 1E 01 00 01 01 F8 FF FF 02 {ff22} 41 4B 4A"));
        assert_eq!(hex(&power_packet(false)), format!("59 44 3E 1E 01 00 01 01 F8 FF FF 03 {ff22} 42 4B 4A"));
        assert_eq!(power_packet(true).len(), 37);
    }

    #[test]
    fn parses_soc_speed_and_state_from_a_valid_frame() {
        let parsed = parse_frame(&frame(160, [0x00, 0x00])).unwrap();
        assert_eq!(parsed.soc1, Some(80.0));
        assert_eq!(parsed.speed, Some(0.0));
        assert_eq!(parsed.state_bytes, [0x01, 0x00, 0x00, 0x00, 0x02, 0x00]);

        let idle = parse_frame(&frame(0xff, [0xff, 0xff])).unwrap();
        assert_eq!(idle.soc1, None);
        assert_eq!(idle.speed, None);
        let moving = parse_frame(&frame(154, [0x7b, 0x00])).unwrap();
        assert!((moving.speed.unwrap() - 12.3).abs() < 1e-9);
        assert_eq!(moving.soc1, Some(77.0));
    }

    #[test]
    fn rejects_a_frame_with_a_bad_checksum_or_length() {
        let mut bad = frame(160, [0, 0]);
        bad[174] = bad[174].wrapping_add(1);
        assert_eq!(parse_frame(&bad), None);
        assert_eq!(parse_frame(&frame(160, [0, 0])[..176]), None);
    }

    #[test]
    fn reassembles_frames_split_across_notifications() {
        let whole = frame(164, [0, 0]);
        let mut buffer = vec![0x00, 0x13];
        buffer.extend_from_slice(&whole[..100]);
        assert!(drain_frames(&mut buffer).is_empty());
        buffer.extend_from_slice(&whole[100..]);
        buffer.extend_from_slice(&whole[..1]);
        let frames = drain_frames(&mut buffer);
        assert_eq!(frames, vec![whole.clone()]);
        assert_eq!(buffer, vec![0x59], "a lone header byte waits for the next notification");
        buffer.extend_from_slice(&whole[1..]);
        assert_eq!(drain_frames(&mut buffer), vec![whole]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn mac_comes_from_the_serial_reversed_or_from_text() {
        assert_eq!(mac_from_serial(&[0x40, 0x15, 0x07, 0x64, 0x3a, 0xc1]), Some(MAC));
        assert_eq!(mac_from_serial(&[0x40, 0x15]), None);
        assert_eq!(parse_mac("C1:3A:64:07:15:40"), Some(MAC));
        assert_eq!(parse_mac("c1-3a-64-07-15-40"), Some(MAC));
        assert_eq!(parse_mac("C1:3A:64"), None);
        assert_eq!(parse_mac("ZZ:3A:64:07:15:40"), None);
    }

    #[test]
    fn only_5a_96_00_accepts_the_pairing() {
        assert!(is_pairing_accepted(&[0x5a, 0x96, 0x00]));
        assert!(!is_pairing_accepted(&[0x5a, 0x96, 0x01]));
    }
}
