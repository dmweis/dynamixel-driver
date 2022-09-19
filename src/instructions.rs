use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, DynamixelDriverError>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DynamixelDriverError {
    #[error("connection timeout")]
    Timeout,
    #[error("{0}")]
    StatusError(StatusError),
    #[error("checksum error on arriving packet")]
    ChecksumError,
    #[error("invalid header")]
    HeaderError,
    #[error("reading error")]
    ReadingError,
    #[error("Failed reading")]
    IoError(#[from] std::io::Error),
    #[error("decoding error for {0}")]
    DecodingError(&'static str),
    #[error("Id mismatch error. Expected {0} got {1}")]
    IdMismatchError(u8, u8),
    #[error("Failed to open serial port")]
    FailedOpeningSerialPort,
}

impl DynamixelDriverError {
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            DynamixelDriverError::Timeout
                | DynamixelDriverError::StatusError(_)
                | DynamixelDriverError::ChecksumError
                | DynamixelDriverError::HeaderError
                | DynamixelDriverError::ReadingError
                | DynamixelDriverError::DecodingError(_)
                | DynamixelDriverError::IdMismatchError(_, _)
        )
    }
}

#[derive(PartialEq, Debug, Eq, Clone)]
pub struct StatusError {
    pub instruction_error: bool,
    pub overload_error: bool,
    pub checksum_error: bool,
    pub range_error: bool,
    pub overheating_error: bool,
    pub angle_limit_error: bool,
    pub input_voltage_error: bool,
}

impl StatusError {
    pub(crate) fn check_error(flag: u8) -> Result<()> {
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
        Err(DynamixelDriverError::StatusError(status_error))
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct Instruction {
    payload: Vec<u8>,
}

impl Instruction {
    pub fn read_instruction(id: u8, addr: u8, length: u8) -> Self {
        let mut data = vec![
            0xFF, // header
            0xFF, id,   // ID
            0x04, // Len
            0x02, // Instruction
            addr, length,
        ];
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        Instruction { payload: data }
    }

    pub fn write_u8(id: u8, addr: u8, data: u8) -> Self {
        let len = 4;
        let mut payload = vec![
            0xFF, // header
            0xFF, id,   // ID
            len,  // Length
            0x03, // Instruction
            addr, data,
        ];
        let checksum = calc_checksum(&payload[2..]);
        payload.push(checksum);
        Instruction { payload }
    }

    pub fn write_u16(id: u8, addr: u8, data: u16) -> Self {
        let len = 5;
        let mut payload = vec![
            0xFF, // header
            0xFF,
            id,   // ID
            len,  // Length
            0x03, // Instruction
            addr,
            data as u8,
            (data >> 8) as u8,
        ];
        let checksum = calc_checksum(&payload[2..]);
        payload.push(checksum);
        Instruction { payload }
    }

    pub fn ping(id: u8) -> Self {
        let mut payload = vec![
            0xFF, // header
            0xFF, id,   // ID
            0x02, // Len
            0x01, // Instruction
        ];
        let checksum = calc_checksum(&payload[2..]);
        payload.push(checksum);
        Instruction { payload }
    }

    pub fn sync_command(addr: u8, data_len: u8, commands: Vec<SyncCommand>) -> Self {
        let len = (data_len + 1) * commands.len() as u8 + 4;
        let mut data = vec![
            0xFF, // header
            0xFF, 0xFE, // Always broadcast ID
            len,  // Len
            0x83, // Instruction
            addr, data_len,
        ];
        // add params
        for entry in &commands {
            data.push(entry.id);
            match data_len {
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
        Instruction { payload: data }
    }

    pub fn serialize(self) -> Vec<u8> {
        self.payload
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct SyncCommand {
    id: u8,
    value: u32,
}

impl SyncCommand {
    pub fn new(id: u8, value: u32) -> SyncCommand {
        SyncCommand { id, value }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn value(&self) -> u32 {
        self.value
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial_driver;
    use crate::*;
    use async_trait::async_trait;
    use serial_driver::{FramedDriver, Status};
    use std::sync::{Arc, Mutex};

    #[test]
    fn read_instruction_serialization() {
        let read = Instruction::read_instruction(1, 43, 1);
        let payload = read.serialize();
        let expected = vec![0xFF_u8, 0xFF, 0x01, 0x04, 0x02, 0x2B, 0x01, 0xCC];
        assert_eq!(payload, expected);
    }

    #[test]
    fn write_instruction_serialization_u8() {
        let write = Instruction::write_u8(0xFE, 0x03, 1);
        let payload = write.serialize();
        let expected = vec![0xFF, 0xFF, 0xFE, 0x04, 0x03, 0x03, 0x01, 0xF6];
        assert_eq!(payload, expected);
    }

    #[test]
    fn ping_serialization() {
        let packet = Instruction::ping(1);
        let payload = packet.serialize();
        assert_eq!(payload, vec![0xFF_u8, 0xFF, 0x01, 0x02, 0x01, 0xFB])
    }

    #[test]
    fn sync_write_serialization_u16() {
        let params = vec![SyncCommand::new(1, 10), SyncCommand::new(2, 10)];
        let packet = Instruction::sync_command(30, 2, params);
        let payload = packet.serialize();
        assert_eq!(
            payload,
            vec![255, 255, 254, 10, 131, 30, 2, 1, 10, 0, 2, 10, 0, 61]
        )
    }

    #[test]
    fn sync_write_serialization_u8() {
        let params = vec![SyncCommand::new(1, 10), SyncCommand::new(2, 10)];
        let packet = Instruction::sync_command(30, 1, params);
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
        let packet = Instruction::sync_command(30, 3, params);
        let _ = packet.serialize();
    }

    struct MockFramedDriver {
        written_data: Arc<Mutex<Vec<Vec<u8>>>>,
        mock_read_data: Vec<Status>,
    }

    impl MockFramedDriver {
        fn new(mock_read_data: Vec<Status>, written_data: Arc<Mutex<Vec<Vec<u8>>>>) -> Self {
            MockFramedDriver {
                written_data,
                mock_read_data,
            }
        }
    }

    #[async_trait]
    impl FramedDriver for MockFramedDriver {
        async fn send(&mut self, message: Instruction) -> Result<()> {
            let payload = message.serialize();
            self.written_data.lock().unwrap().push(payload);
            Ok(())
        }

        async fn receive(&mut self) -> Result<Status> {
            Ok(self.mock_read_data.remove(0))
        }
    }

    #[tokio::test]
    async fn sync_write_compliance_margin_writes() {
        let writing_buffer = Arc::new(Mutex::new(vec![]));
        let mock_port = MockFramedDriver::new(vec![], writing_buffer.clone());
        let mut driver = DynamixelDriver::with_driver(Box::new(mock_port));
        let commands = vec![(1_u8, 0_u32), (2, 0), (3, 0), (4, 0)];
        driver
            .sync_write_compliance_margin_both(commands)
            .await
            .unwrap();

        let mut writing_buffer_guard = writing_buffer.lock().unwrap();
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 254, 12, 131, 26, 1, 1, 0, 2, 0, 3, 0, 4, 0, 77]
        );
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 254, 12, 131, 27, 1, 1, 0, 2, 0, 3, 0, 4, 0, 76]
        );
        assert!(writing_buffer_guard.is_empty());
    }

    #[tokio::test]
    async fn sync_write_compliance_slope_writes() {
        let writing_buffer = Arc::new(Mutex::new(vec![]));
        let mock_port = MockFramedDriver::new(vec![], writing_buffer.clone());
        let mut driver = DynamixelDriver::with_driver(Box::new(mock_port));
        let commands = vec![(1_u8, 0_u32), (2, 0), (3, 0), (4, 0)];
        driver
            .sync_write_compliance_slope_both(commands)
            .await
            .unwrap();

        let mut writing_buffer_guard = writing_buffer.lock().unwrap();
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 254, 12, 131, 28, 1, 1, 0, 2, 0, 3, 0, 4, 0, 75]
        );
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 254, 12, 131, 29, 1, 1, 0, 2, 0, 3, 0, 4, 0, 74]
        );
        assert!(writing_buffer_guard.is_empty());
    }

    #[tokio::test]
    async fn sync_write_positions_writes() {
        let writing_buffer = Arc::new(Mutex::new(vec![]));
        let mock_port = MockFramedDriver::new(vec![], writing_buffer.clone());
        let mut driver = DynamixelDriver::with_driver(Box::new(mock_port));
        let commands = vec![(1_u8, 0_u32), (2, 0), (3, 0), (4, 0)];
        driver.sync_write_position(commands).await.unwrap();
        let mut writing_buffer_guard = writing_buffer.lock().unwrap();
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 254, 16, 131, 30, 2, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 68]
        );
        assert!(writing_buffer_guard.is_empty());
    }

    #[tokio::test]
    async fn write_positions_writes() {
        let writing_buffer = Arc::new(Mutex::new(vec![]));
        let mock_port = MockFramedDriver::new(vec![Status::new(1, vec![])], writing_buffer.clone());
        let mut driver = DynamixelDriver::with_driver(Box::new(mock_port));
        driver.write_position(1, 150).await.unwrap();
        let mut writing_buffer_guard = writing_buffer.lock().unwrap();
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 1, 5, 3, 30, 150, 0, 66]
        );
        assert!(writing_buffer_guard.is_empty());
    }

    #[tokio::test]
    async fn sync_write_torque_writes() {
        let writing_buffer = Arc::new(Mutex::new(vec![]));
        let mock_port = MockFramedDriver::new(vec![Status::new(1, vec![])], writing_buffer.clone());
        let mut driver = DynamixelDriver::with_driver(Box::new(mock_port));
        let input = vec![(1, 0), (2, 0), (3, 1), (4, 1)];
        driver.sync_write_torque(input).await.unwrap();
        let mut writing_buffer_guard = writing_buffer.lock().unwrap();
        assert_eq!(
            writing_buffer_guard.remove(0),
            vec![255, 255, 254, 12, 131, 24, 1, 1, 0, 2, 0, 3, 1, 4, 1, 77]
        );
        assert!(writing_buffer_guard.is_empty());
    }
}
