mod dynamixel_port;

use dynamixel_port::{DynamixelPort, Instruction, calc_checksum};
#[cfg(test)]
use dynamixel_port::DynamixelConnection;

use std::error::Error;

// EEPROM table
#[allow(dead_code)]
const MODEL_NUMBER: u8 = 0;
#[allow(dead_code)]
const FIRMWARE_VERSION: u8 = 2;
const ID: u8 = 3;
#[allow(dead_code)]
const BAUD_RATE: u8 = 4;
const MAX_TORQUE: u8 = 14;

// RAM table
const TORQUE_ENABLED: u8 = 24;
const CW_COMPLIANCE_SLOPE: u8 = 28;
const CWW_COMPLIANCE_SLOPE: u8 = 29;
const GOAL_POSITION: u8 = 30;
const MOVING_SPEED: u8 = 32;
const PRESENT_POSITION: u8 = 36;
const PRESENT_TEMPERATURE: u8 = 43;
const PRESENT_VOLTAGE: u8 = 42;

#[derive(Debug, PartialEq, Clone, Copy)]
struct Ping {
    id: u8
}

impl Ping {
    fn new(id: u8) -> Ping {
        Ping {
            id
        }
    }
}

impl Instruction for Ping {
    fn serialize(&self) -> Vec<u8> {
        let mut data = vec![
            0xFF, // header
            0xFF,
            self.id, // ID
            0x02, // Len
            0x01 // Instruction
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
}

impl From<(u8, f32)> for SyncCommandFloat {
    fn from(input: (u8, f32)) -> Self {
        let (id, val) = input;
        SyncCommandFloat::new(id, val)
    }
}

#[derive(Debug, Clone)]
struct SyncWrite {
    addr: u8,
    data_len: u8,
    data: Vec<SyncCommand>,
}

impl SyncWrite {
    fn new(addr: u8, data_len: u8, data: Vec<SyncCommand>) -> SyncWrite {
        SyncWrite { addr, data_len, data }
    }
}

impl Instruction for SyncWrite {
    fn serialize(&self) -> Vec<u8> {
        let len = (self.data_len + 1) * self.data.len() as u8 + 4;
        let mut data = vec![
            0xFF, // header
            0xFF,
            0xFE, // Always broadcast ID
            len, // Len
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
                },
                2 => {
                    data.push(entry.value as u8);
                    data.push((entry.value >> 8) as u8);
                },
                _ => unimplemented!("Sync write only implement for u8 and u16")
            }
        }
        let checksum = calc_checksum(&data[2..]);
        data.push(checksum);
        data
    }
}

pub struct DynamixelDriver {
    port: DynamixelPort
}

impl DynamixelDriver {
    pub fn new(port_name: &str) -> Result<DynamixelDriver, Box<dyn Error>> {
        Ok(DynamixelDriver {
            port: DynamixelPort::with_serial_port(port_name)?
        })
    }

    #[cfg(test)]
    fn new_with_connection(connection: Box<impl DynamixelConnection + 'static>) -> DynamixelDriver {
        DynamixelDriver {
            port: DynamixelPort::new(connection)
        }
    }

    pub fn ping(&mut self, id: u8) -> Result<(), Box<dyn Error>> {
        let ping = Ping::new(id);
        self.port.write_message(&ping)?;
        let _status = self.port.read_message()?;
        Ok(())
    }

    pub fn write_id(&mut self, id: u8, new_id: u8) -> Result<(), Box<dyn Error>> {
        self.port.write_u8(id, ID, new_id)?;
        Ok(())
    }

    pub fn write_torque(&mut self, id: u8, torque_enabled: bool) -> Result<(), Box<dyn Error>> {
        if torque_enabled {
            Ok(self.port.write_u8(id, TORQUE_ENABLED, 1)?)
        } else {
            Ok(self.port.write_u8(id, TORQUE_ENABLED, 0)?)
        }
    }

    pub fn read_temperature(&mut self, id: u8) -> Result<u8, Box<dyn Error>> {
        Ok(self.port.read_u8(id, PRESENT_TEMPERATURE)?)
    }

    pub fn read_voltage(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        Ok(self.port.read_u8(id, PRESENT_VOLTAGE)? as f32 / 10.0)
    }

    pub fn read_position(&mut self, id: u8) -> Result<u16, Box<dyn Error>> {
        let position = self.port.read_u16(id, PRESENT_POSITION)?;
        Ok(position)
    }

    pub fn read_position_degrees(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let position = self.port.read_u16(id, PRESENT_POSITION)? as f32;
        let position = position / 3.41;
        Ok(position)
    }

    pub fn read_position_rad(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let pos_rad = self.read_position_degrees(id)?.to_radians();
        Ok(pos_rad)
    }

    pub fn write_compliance_slope_both(&mut self, id: u8, compliance: u8) -> Result<(), Box<dyn Error>> {
        self.port.write_u8(id, CW_COMPLIANCE_SLOPE, compliance)?;
        self.port.write_u8(id, CWW_COMPLIANCE_SLOPE, compliance)?;
        Ok(())
    }

    pub fn sync_write_compliance_both<T: Into<SyncCommand>>(&mut self, compliance: Vec<T>) -> Result<(), Box<dyn Error>> {
        let compliance: Vec<SyncCommand> = compliance
            .into_iter()
            .map(|command| command.into())
            .collect();
        let message_cw = SyncWrite::new(CW_COMPLIANCE_SLOPE, 1, compliance.clone());
        let message_cww = SyncWrite::new(CWW_COMPLIANCE_SLOPE, 1, compliance);
        self.port.write_message(&message_cw)?;
        self.port.write_message(&message_cww)?;
        Ok(())
    }

    pub fn sync_write_torque<T: Into<SyncCommand>>(&mut self, torque: Vec<T>) -> Result<(), Box<dyn Error>> {
        let torque_commands: Vec<SyncCommand> = torque
            .into_iter()
            .map(|command| command.into())
            .collect();
        let torque_message = SyncWrite::new(TORQUE_ENABLED, 1, torque_commands);
        self.port.write_message(&torque_message)?;
        Ok(())
    }

    pub fn write_position(&mut self, id: u8, pos: u16) -> Result<(), Box<dyn Error>> {
        self.port.write_u16(id, GOAL_POSITION, pos)?;
        Ok(())
    }

    pub fn write_position_degrees(&mut self, id: u8, pos: f32) -> Result<(), Box<dyn Error>> {
        let goal_position = ((pos*3.41) as i32) as u16;
        self.port.write_u16(id, GOAL_POSITION, goal_position)?;
        Ok(())
    }

    pub fn write_position_rad(&mut self, id: u8, pos: f32) -> Result<(), Box<dyn Error>> {
        self.write_position_degrees(id, pos.to_degrees())?;
        Ok(())
    }

    pub fn sync_write_position<T: Into<SyncCommand>>(&mut self, positions: Vec<T>) -> Result<(), Box<dyn Error>> {
        let positions: Vec<SyncCommand> = positions
            .into_iter()
            .map(|command| command.into())
            .collect();
        let message = SyncWrite::new(GOAL_POSITION, 2, positions);
        self.port.write_message(&message)?;
        Ok(())
    }

    pub fn sync_write_position_degrees(&mut self, positions: Vec<SyncCommandFloat>) -> Result<(), Box<dyn Error>> {
        let positions_dyn_units: Vec<SyncCommand> = positions
                .into_iter()
                .map(|command| {
                    let goal_position = ((command.value*3.41) as i32) as u32;
                    SyncCommand::new(command.id, goal_position)
                }).collect();
        let message = SyncWrite::new(GOAL_POSITION, 2, positions_dyn_units);
        self.port.write_message(&message)?;
        Ok(())
    }

    pub fn sync_write_position_rad(&mut self, positions: Vec<SyncCommandFloat>) -> Result<(), Box<dyn Error>> {
        let positions_degrees: Vec<SyncCommandFloat> = positions
                .into_iter()
                .map(|command| {
                    SyncCommandFloat::new(command.id, command.value.to_degrees())
                }).collect();
        self.sync_write_position_degrees(positions_degrees)?;
        Ok(())
    }

    pub fn sync_write_moving_speed<T: Into<SyncCommand>>(&mut self, speeds: Vec<T>) -> Result<(), Box<dyn Error>> {
        let speeds: Vec<SyncCommand> = speeds
            .into_iter()
            .map(|command| command.into())
            .collect();
        let message = SyncWrite::new(MOVING_SPEED, 2, speeds);
        self.port.write_message(&message)?;
        Ok(())
    }

    pub fn read_max_torque(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let max_torque = self.port.read_u16(id, MAX_TORQUE)? as f32;
        let max_torque_percentage = max_torque / 2013.0;
        Ok(max_torque_percentage)
    }

    pub fn search_all(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut ids = vec![];
        for i in 1..254 {
            if self.ping(i).is_ok() {
                ids.push(i);
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamixel_port::*;
    use std::sync::mpsc::{ channel, Sender };

    #[test]
    fn ping_serialization() {
        let packet = Ping::new(1);
        let payload = packet.serialize();
        assert_eq!(payload, vec![0xFF_u8,0xFF,0x01,0x02,0x01,0xFB])
    }

    #[test]
    fn sync_write_serialization_u16() {
        let params = vec![
            SyncCommand::new(1, 10),
            SyncCommand::new(2, 10),
        ];
        let packet = SyncWrite::new(30, 2, params);
        let payload = packet.serialize();
        assert_eq!(payload, vec![255, 255, 254, 10, 131, 30, 2, 1, 10, 0, 2, 10, 0, 61])
    }

    #[test]
    fn sync_write_serialization_u8() {
        let params = vec![
            SyncCommand::new(1, 10),
            SyncCommand::new(2, 10),
        ];
        let packet = SyncWrite::new(30, 1, params);
        let payload = packet.serialize();
        assert_eq!(payload, vec![255, 255, 254, 8, 131, 30, 1, 1, 10, 2, 10, 64])
    }

    #[test]
    #[should_panic(expected = "not implemented: Sync write only implement for u8 and u16")]
    fn sync_write_serialization_fail() {
        let params = vec![
            SyncCommand::new(1, 10),
            SyncCommand::new(2, 10),
        ];
        let packet = SyncWrite::new(30, 3, params);
        let _ = packet.serialize();
    }

    struct MockSerialPort {
        written_data: Sender<Vec<u8>>,
        mock_read_data: Vec<Status>
    }

    impl MockSerialPort {
        fn new(mock_read_data: Vec<Status>, written_data: Sender<Vec<u8>>) -> MockSerialPort {
            MockSerialPort {
                written_data,
                mock_read_data,
            }
        }
    }

    impl DynamixelConnection for MockSerialPort {
        fn write_message(&mut self, message: &dyn Instruction) -> Result<(), Box<dyn Error>> {
            let payload = message.serialize();
            self.written_data.send(payload).unwrap();
            Ok(())
        }
    
        fn read_message(&mut self) -> Result<Status, Box<dyn Error>> {
            Ok(self.mock_read_data.remove(0))
        }
    }

    #[test]
    fn sync_write_compliance_writes() {
        let (tx, rx) = channel();
        let mock_port = MockSerialPort::new(vec![], tx);
        let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
        let commands = vec![
            (1_u8, 0_u32),
            (2, 0),
            (3, 0),
            (4, 0),
        ];
        driver.sync_write_compliance_both(commands).unwrap();
        assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 12, 131, 28, 1, 1, 0, 2, 0, 3, 0, 4, 0, 75]);
        assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 12, 131, 29, 1, 1, 0, 2, 0, 3, 0, 4, 0, 74]);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn sync_write_positions_writes() {
        let (tx, rx) = channel();
        let mock_port = MockSerialPort::new(vec![], tx);
        let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
        let commands = vec![
            (1_u8, 0_u32),
            (2, 0),
            (3, 0),
            (4, 0),
        ];
        driver.sync_write_position(commands).unwrap();
        assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 254, 16, 131, 30, 2, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 68]);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn write_positions_writes() {
        let (tx, rx) = channel();
        let mock_port = MockSerialPort::new(vec![
            Status::new(1, vec![]),
        ], tx);
        let mut driver = DynamixelDriver::new_with_connection(Box::new(mock_port));
        driver.write_position(1, 150).unwrap();
        assert_eq!(rx.try_recv().unwrap(), vec![255, 255, 1, 5, 3, 30, 150, 0, 66]);
        assert!(rx.try_recv().is_err());
    }
}
