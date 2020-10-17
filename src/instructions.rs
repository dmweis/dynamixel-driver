use std::{error::Error, io};

#[derive(PartialEq, Debug)]
pub(crate) struct StatusError {
    instruction_error: bool,
    overload_error: bool,
    checksum_error: bool,
    range_error: bool,
    overheating_error: bool,
    angle_limit_error: bool,
    input_voltage_error: bool,
}

impl StatusError {
    pub(crate) fn check_error(flag: u8) -> Result<(), io::Error> {
        if flag == 0 {
            return Ok(());
        }
        let status_error = StatusError {
            input_voltage_error: flag & (1 << 0) != 0,
            angle_limit_error: flag & (1 << 1) != 0,
            overheating_error: flag & (1 << 2) != 0,
            range_error: flag & (1 << 3) != 0,
            checksum_error: flag & (1 << 4) != 0,
            overload_error: flag & (1 << 5) != 0,
            instruction_error: flag & (1 << 6) != 0,
        };
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("{}", status_error),
        ))
    }
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut description = String::new();
        if self.input_voltage_error {
            description.push_str("input_voltage_error ");
        }
        if self.angle_limit_error {
            description.push_str("angle_limit_error ");
        }
        if self.overheating_error {
            description.push_str("overheating_error ");
        }
        if self.range_error {
            description.push_str("range_error ");
        }
        if self.checksum_error {
            description.push_str("checksum_error ");
        }
        if self.overload_error {
            description.push_str("overload_error ");
        }
        if self.instruction_error {
            description.push_str("instruction_error ");
        }
        write!(f, "{}", description)
    }
}

pub(crate) fn calc_checksum(payload: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for b in payload {
        sum = sum.wrapping_add(*b);
    }
    !sum
}

pub(crate) trait Instruction: Send {
    fn serialize(&self) -> Vec<u8>;
}

pub(crate) struct ReadInstruction {
    id: u8,
    addr: u8,
    length: u8,
}

impl ReadInstruction {
    pub(crate) fn new(id: u8, addr: u8, length: u8) -> ReadInstruction {
        ReadInstruction { id, addr, length }
    }
}

impl Instruction for ReadInstruction {
    fn serialize(&self) -> Vec<u8> {
        let mut data = vec![
            0xFF, // header
            0xFF,
            self.id, // ID
            0x04,    // Len
            0x02,    // Instruction
            self.addr,
            self.length,
        ];
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

pub(crate) struct WriteInstruction {
    id: u8,
    addr: u8,
    payload: Vec<u8>,
}

impl WriteInstruction {
    pub(crate) fn with_u8(id: u8, addr: u8, data: u8) -> WriteInstruction {
        WriteInstruction {
            id,
            addr,
            payload: vec![data],
        }
    }

    pub(crate) fn with_u16(id: u8, addr: u8, data: u16) -> WriteInstruction {
        let a = (data >> 8) as u8;
        let b = data as u8;
        WriteInstruction {
            id,
            addr,
            payload: vec![b, a],
        }
    }
}

impl Instruction for WriteInstruction {
    fn serialize(&self) -> Vec<u8> {
        let len = (self.payload.len() + 3) as u8;
        let mut data = vec![
            0xFF, // header
            0xFF, self.id, // ID
            len,     // Length
            0x03,    // Instruction
            self.addr,
        ];
        data.extend(self.payload.iter());
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) struct Ping {
    id: u8,
}

impl Ping {
    pub(crate) fn new(id: u8) -> Ping {
        Ping { id }
    }
}

impl Instruction for Ping {
    fn serialize(&self) -> Vec<u8> {
        let mut data = vec![
            0xFF, // header
            0xFF, self.id, // ID
            0x02,    // Len
            0x01,    // Instruction
        ];
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SyncCommand {
    id: u8,
    value: u32,
}

impl SyncCommand {
    pub fn new(id: u8, value: u32) -> SyncCommand {
        SyncCommand { id, value }
    }
}

impl From<(u8, u32)> for SyncCommand {
    fn from(input: (u8, u32)) -> Self {
        let (id, val) = input;
        SyncCommand::new(id, val)
    }
}

impl From<(u8, bool)> for SyncCommand {
    fn from(input: (u8, bool)) -> Self {
        let (id, val) = input;
        SyncCommand::new(id, val as u32)
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SyncCommandFloat {
    id: u8,
    value: f32,
}

impl SyncCommandFloat {
    pub fn new(id: u8, value: f32) -> SyncCommandFloat {
        SyncCommandFloat { id, value }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn value(&self) -> f32 {
        self.value
    }
}

impl From<(u8, f32)> for SyncCommandFloat {
    fn from(input: (u8, f32)) -> Self {
        let (id, val) = input;
        SyncCommandFloat::new(id, val)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SyncWrite {
    addr: u8,
    data_len: u8,
    data: Vec<SyncCommand>,
}

impl SyncWrite {
    pub(crate) fn new(addr: u8, data_len: u8, data: Vec<SyncCommand>) -> SyncWrite {
        SyncWrite {
            addr,
            data_len,
            data,
        }
    }
}

impl Instruction for SyncWrite {
    fn serialize(&self) -> Vec<u8> {
        let len = (self.data_len + 1) * self.data.len() as u8 + 4;
        let mut data = vec![
            0xFF, // header
            0xFF,
            0xFE, // Always broadcast ID
            len,  // Len
            0x83, // Instruction
            self.addr,
            self.data_len,
        ];
        // add params
        for entry in &self.data {
            data.push(entry.id);
            match self.data_len {
                1 => {
                    data.push(entry.value as u8);
                }
                2 => {
                    data.push(entry.value as u8);
                    data.push((entry.value >> 8) as u8);
                }
                _ => {
                    unimplemented!("Sync write only implement for u8 and u16");
                }
            }
        }
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_instruction_serialization() {
        let read = ReadInstruction::new(1, 43, 1);
        let payload = read.serialize();
        let expected = vec![0xFF_u8, 0xFF, 0x01, 0x04, 0x02, 0x2B, 0x01, 0xCC];
        assert_eq!(payload, expected);
    }

    #[test]
    fn write_instruction_serialization_u8() {
        let write = WriteInstruction::with_u8(0xFE, 0x03, 1);
        let payload = write.serialize();
        let expected = vec![0xFF, 0xFF, 0xFE, 0x04, 0x03, 0x03, 0x01, 0xF6];
        assert_eq!(payload, expected);
    }

    #[test]
    fn ping_serialization() {
        let packet = Ping::new(1);
        let payload = packet.serialize();
        assert_eq!(payload, vec![0xFF_u8, 0xFF, 0x01, 0x02, 0x01, 0xFB])
    }

    #[test]
    fn sync_write_serialization_u16() {
        let params = vec![SyncCommand::new(1, 10), SyncCommand::new(2, 10)];
        let packet = SyncWrite::new(30, 2, params);
        let payload = packet.serialize();
        assert_eq!(
            payload,
            vec![255, 255, 254, 10, 131, 30, 2, 1, 10, 0, 2, 10, 0, 61]
        )
    }

    #[test]
    fn sync_write_serialization_u8() {
        let params = vec![SyncCommand::new(1, 10), SyncCommand::new(2, 10)];
        let packet = SyncWrite::new(30, 1, params);
        let payload = packet.serialize();
        assert_eq!(
            payload,
            vec![255, 255, 254, 8, 131, 30, 1, 1, 10, 2, 10, 64]
        )
    }

    #[test]
    #[should_panic(expected = "not implemented: Sync write only implement for u8 and u16")]
    fn sync_write_serialization_fail() {
        let params = vec![SyncCommand::new(1, 10), SyncCommand::new(2, 10)];
        let packet = SyncWrite::new(30, 3, params);
        let _ = packet.serialize();
    }

    // struct MockSerialPort {
    //     written_data: Sender<Vec<u8>>,
    //     mock_read_data: Vec<Status>,
    // }

    //     impl MockSerialPort {
    //         fn new(mock_read_data: Vec<Status>, written_data: Sender<Vec<u8>>) -> MockSerialPort {
    //             MockSerialPort {
    //                 written_data,
    //                 mock_read_data,
    //             }
    //         }
    //     }

    //     impl DynamixelConnection for MockSerialPort {
    //         fn flush(&mut self) -> Result<(), Box<dyn Error>> {
    //             Ok(())
    //         }

    //         fn write_message(&mut self, message: &dyn Instruction) -> Result<(), Box<dyn Error>> {
    //             let payload = message.serialize();
    //             self.written_data.send(payload).unwrap();
    //             Ok(())
    //         }

    //         fn read_message(&mut self) -> Result<Status, Box<dyn Error>> {
    //             Ok(self.mock_read_data.remove(0))
    //         }
    //     }

    //     #[test]
    //     fn sync_write_compliance_writes() {
    //         let (tx, rx) = channel();
    //         let mock_port = MockSerialPort::new(vec![], tx);
    //         let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
    //         let commands = vec![
    //             (1_u8, 0_u32),
    //             (2, 0),
    //             (3, 0),
    //             (4, 0),
    //         ];
    //         driver.sync_write_compliance_both(commands).unwrap();
    //         assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 12, 131, 28, 1, 1, 0, 2, 0, 3, 0, 4, 0, 75]);
    //         assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 12, 131, 29, 1, 1, 0, 2, 0, 3, 0, 4, 0, 74]);
    //         assert!(rx.try_recv().is_err());
    //     }

    //     #[test]
    //     fn sync_write_positions_writes() {
    //         let (tx, rx) = channel();
    //         let mock_port = MockSerialPort::new(vec![], tx);
    //         let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
    //         let commands = vec![
    //             (1_u8, 0_u32),
    //             (2, 0),
    //             (3, 0),
    //             (4, 0),
    //         ];
    //         driver.sync_write_position(commands).unwrap();
    //         assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 16, 131, 30, 2, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 68]);
    //         assert!(rx.try_recv().is_err());
    //     }

    //     #[test]
    //     fn write_positions_writes() {
    //         let (tx, rx) = channel();
    //         let mock_port = MockSerialPort::new(vec![
    //             Status::new(1, vec![]),
    //         ], tx);
    //         let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
    //         driver.write_position(1, 150).unwrap();
    //         assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 1, 5, 3, 30, 150, 0, 66]);
    //         assert!(rx.try_recv().is_err());
    //     }

    //     #[test]
    //     fn sync_write_torque_writes() {
    //         let (tx, rx) = channel();
    //         let mock_port = MockSerialPort::new(vec![], tx);
    //         let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
    //         let input = vec![
    //             (1, 0),
    //             (2, 0),
    //             (3, 1),
    //             (4, 1),
    //         ];
    //         driver.sync_write_torque(input).unwrap();
    //         assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 12, 131, 24, 1, 1, 0, 2, 0, 3, 1, 4, 1, 77]);
    //         assert!(rx.try_recv().is_err());
    //     }
}
