mod dynamixel_port;

use dynamixel_port::{DynamixelPort, Instruction, calc_checksum};

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

pub struct SyncCommand {
    id: u8,
    value: u32,
}

impl SyncCommand {
    pub fn new(id: u8, value: u32) -> SyncCommand {
        SyncCommand { id, value }
    }
}

pub struct SyncCommandFloat {
    id: u8,
    value: f32,
}

impl SyncCommandFloat {
    pub fn new(id: u8, value: f32) -> SyncCommandFloat {
        SyncCommandFloat { id, value }
    }
}

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
            port: DynamixelPort::new(port_name)?
        })
    }

    pub fn ping(&mut self, id: u8) -> Result<(), Box<dyn Error>> {
        let ping = Ping::new(id);
        self.port.write_message(ping)?;
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

    pub fn read_voltage(&mut self, id: u8) -> Result<u8, Box<dyn Error>> {
        Ok(self.port.read_u8(id, PRESENT_VOLTAGE)?)
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

    pub fn sync_write_position(&mut self, positions: Vec<SyncCommand>) -> Result<(), Box<dyn Error>> {
        let message = SyncWrite::new(GOAL_POSITION, 2, positions);
        self.port.write_message(message)?;
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
        self.port.write_message(message)?;
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

    pub fn sync_write_moving_speed(&mut self, speeds: Vec<SyncCommand>) -> Result<(), Box<dyn Error>> {
        let message = SyncWrite::new(MOVING_SPEED, 2, speeds);
        self.port.write_message(message)?;
        Ok(())
    }

    pub fn read_max_torque(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let max_torque = self.port.read_u16(id, MAX_TORQUE)? as f32;
        let max_torque_percentage = max_torque / 2013.0;
        Ok(max_torque_percentage)
    }

    pub fn search_all(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut ids = vec![];
        for i in 1..255 {
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

    #[test]
    fn ping_serialization() {
        let packet = Ping::new(1);
        let payload = packet.serialize();
        assert_eq!(payload, vec![0xFF_u8,0xFF,0x01,0x02,0x01,0xFB])
    }

    #[test]
    fn sync_write_serialization() {
        let params = vec![
            SyncCommand::new(1, 10),
            SyncCommand::new(2, 10),
        ];
        let packet = SyncWrite::new(30, 2, params);
        let payload = packet.serialize();
        assert_eq!(payload, vec![255, 255, 254, 10, 131, 30, 2, 1, 10, 0, 2, 10, 0, 61])
    }
}
