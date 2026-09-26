use btleplug::api::bleuuid::uuid_from_u16;
use uuid::Uuid;

pub const SERVICE_UUID: Uuid = uuid_from_u16(0xff00);
pub const SERVICE_UUID_FULL: Uuid =
    Uuid::from_u128(0x0000ff00_0000_1000_8000_00805f9b34fb);
pub const NOTIFY_CHARACTERISTIC_UUID: Uuid = uuid_from_u16(0xff01);
pub const WRITE_CHARACTERISTIC_UUID: Uuid = uuid_from_u16(0xff02);
pub const WRITE_CHARACTERISTIC_ALT_UUID: Uuid = uuid_from_u16(0xff03);

pub const FRAME_START: u8 = 0xdd;
pub const FRAME_END: u8 = 0x77;

/// JBD Basic Pack Status read: DD A5 03 00 FF FD 77
pub const STATUS_COMMAND: [u8; 7] = [0xdd, 0xa5, 0x03, 0x00, 0xff, 0xfd, 0x77];
/// JBD cell voltage read: DD A5 04 00 FF FC 77
pub const CELL_VOLTAGE_COMMAND: [u8; 7] = [0xdd, 0xa5, 0x04, 0x00, 0xff, 0xfc, 0x77];

pub const COMMAND_BASIC_STATUS: u8 = 0x03;
pub const COMMAND_CELL_VOLTAGE: u8 = 0x04;

#[derive(Debug, Clone, PartialEq)]
pub struct BasicStatus {
    pub voltage: f64,
    pub current: f64,
    pub remaining_capacity: f64,
    pub rated_capacity: f64,
    pub cycles: u16,
    pub production_date: String,
    pub balance_status: u16,
    pub protection_status: u16,
    pub soc: u8,
    pub fet_status: u8,
    pub temperatures: Vec<f64>,
}

fn u16_be(data: &[u8], offset: usize) -> u16 {
    ((data[offset] as u16) << 8) | data[offset + 1] as u16
}

/// Pulls complete JBD frames out of the accumulated BLE notification buffer.
/// A frame starts with 0xDD, declares its payload length at byte 3 (total
/// length = payload + 7) and ends with 0x77. Mirrors `index.html`.
pub fn drain_complete_frames(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    loop {
        if buffer.is_empty() {
            break;
        }
        let expected = if buffer.len() >= 4 {
            buffer[3] as usize + 7
        } else {
            0
        };
        if expected < 7
            || buffer[0] != FRAME_START
            || buffer.len() < expected
            || buffer[expected - 1] != FRAME_END
        {
            break;
        }
        frames.push(buffer.drain(..expected).collect());
    }
    frames
}

/// Port of the Basic Pack Status (`0x03`) parsing in `index.html`.
pub fn parse_basic_status(frame: &[u8]) -> Option<BasicStatus> {
    if frame.len() < 27 || frame[0] != FRAME_START || frame[1] != COMMAND_BASIC_STATUS {
        return None;
    }
    let production_value = u16_be(frame, 14);
    let year = 2000 + (production_value >> 9);
    let month = (production_value >> 5) & 0x0f;
    let day = production_value & 0x1f;
    let temperature_count = frame[26] as usize;
    let mut temperatures = Vec::new();
    for index in 0..temperature_count {
        let offset = 27 + index * 2;
        if offset + 1 >= frame.len() - 3 {
            break;
        }
        temperatures.push(u16_be(frame, offset) as f64 / 10.0 - 273.15);
    }
    Some(BasicStatus {
        voltage: u16_be(frame, 4) as f64 / 100.0,
        current: u16_be(frame, 6) as i16 as f64 / 100.0,
        remaining_capacity: u16_be(frame, 8) as f64 / 100.0,
        rated_capacity: u16_be(frame, 10) as f64 / 100.0,
        cycles: u16_be(frame, 12),
        production_date: format!("{year:04}-{month:02}-{day:02}"),
        balance_status: u16_be(frame, 16),
        protection_status: u16_be(frame, 18),
        soc: frame[23],
        fet_status: frame[24],
        temperatures,
    })
}

/// Port of the cell voltage (`0x04`) parsing in `index.html`.
pub fn parse_cell_voltages(frame: &[u8]) -> Option<Vec<f64>> {
    if frame.len() < 6 || frame[0] != FRAME_START || frame[1] != COMMAND_CELL_VOLTAGE {
        return None;
    }
    let cell_count = frame[3] as usize / 2;
    let mut voltages = Vec::with_capacity(cell_count);
    for index in 0..cell_count {
        let offset = 4 + index * 2;
        if offset + 1 >= frame.len() {
            break;
        }
        voltages.push(u16_be(frame, offset) as f64 / 1000.0);
    }
    Some(voltages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(command: u8) -> Vec<u8> {
        let mut frame = vec![FRAME_START, command, 0x00, 0x00];
        frame.push(0x77);
        frame
    }

    #[test]
    fn assembles_multiple_frames_from_stream() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&STATUS_COMMAND);
        stream.extend_from_slice(&STATUS_COMMAND);
        let frames = drain_complete_frames(&mut stream);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], STATUS_COMMAND);
        assert!(stream.is_empty());
    }

    #[test]
    fn waits_for_complete_frame() {
        let mut stream = vec![FRAME_START, 0x03, 0x00, 0x19];
        assert!(drain_complete_frames(&mut stream).is_empty());

        let mut packet = vec![0u8; 32];
        packet[0] = FRAME_START;
        packet[1] = 0x03;
        packet[2] = 0x00;
        packet[3] = 0x19;
        packet[31] = FRAME_END;
        stream.extend_from_slice(&packet[4..]);

        let frames = drain_complete_frames(&mut stream);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], packet);
        assert!(stream.is_empty());
    }

    #[test]
    fn parses_basic_status_offsets() {
        // payload length 33 -> total frame length 40
        let mut packet = vec![0u8; 40];
        packet[0] = 0xdd;
        packet[1] = 0x03;
        packet[2] = 0x00;
        packet[3] = 33;
        packet[4] = 0x14;
        packet[5] = 0x79; // 0x1479 = 5241 -> 52.41 V
        packet[6] = 0xfe;
        packet[7] = 0x0c; // -500 -> -5.00 A
        packet[8] = 0x27;
        packet[9] = 0x10; // 10000 -> 100.00 Ah
        packet[10] = 0x4e;
        packet[11] = 0x20; // 20000 -> 200.00 Ah
        packet[12] = 0x00;
        packet[13] = 0x2a; // 42 cycles
        packet[14] = 0x30; // (24<<9) | (4<<5) | 12 = 12428 -> 2024-04-12
        packet[15] = 0x8c;
        packet[16] = 0x00;
        packet[17] = 0x00; // balance 0x0000
        packet[18] = 0x00;
        packet[19] = 0x01; // protection 0x0001
        packet[23] = 88; // soc
        packet[24] = 0x03; // fet status
        packet[26] = 3; // temperature count
        packet[27] = 0x0b;
        packet[28] = 0xa9; // 2985 -> 25.35 C
        packet[29] = 0x0b;
        packet[30] = 0xb9; // 3001 -> 26.95 C
        packet[31] = 0x0a;
        packet[32] = 0xf1; // 2801 -> 6.95 C
        packet[39] = 0x77;

        let status = parse_basic_status(&packet).unwrap();
        assert_eq!(status.voltage, 52.41);
        assert_eq!(status.current, -5.0);
        assert_eq!(status.remaining_capacity, 100.0);
        assert_eq!(status.rated_capacity, 200.0);
        assert_eq!(status.cycles, 42);
        assert_eq!(status.production_date, "2024-04-12");
        assert_eq!(status.balance_status, 0x0000);
        assert_eq!(status.protection_status, 0x0001);
        assert_eq!(status.soc, 88);
        assert_eq!(status.fet_status, 0x03);
        assert_eq!(status.temperatures.len(), 3);
        assert!((status.temperatures[0] - 25.35).abs() < 1e-9);
        assert!((status.temperatures[1] - 26.95).abs() < 1e-9);
        assert!((status.temperatures[2] - 6.95).abs() < 1e-9);
    }

    #[test]
    fn parses_cell_voltages() {
        let mut packet = vec![0u8; 10];
        packet[0] = 0xdd;
        packet[1] = 0x04;
        packet[2] = 0x00;
        packet[3] = 4; // 2 cells
        packet[4] = 0x0e;
        packet[5] = 0x7c; // 0x0e7c = 3708 -> 3.708 V
        packet[6] = 0x0e;
        packet[7] = 0x80; // 3712 -> 3.712 V
        packet[9] = 0x77;
        let voltages = parse_cell_voltages(&packet).unwrap();
        assert_eq!(voltages, vec![3.708, 3.712]);
    }

    #[test]
    fn rejects_wrong_command() {
        assert!(parse_basic_status(&frame(0x04)).is_none());
        assert!(parse_cell_voltages(&frame(0x03)).is_none());
    }
}