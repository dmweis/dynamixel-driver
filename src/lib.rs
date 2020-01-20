mod dynamixel_port;

use dynamixel_port::{DynamixelPort, Instruction, calc_checksum};

use std::error::Error;

// EEPROM table
#[allow(dead_code)]
const MODEL_NUMBER: u8 = 0;
#[allow(dead_code)]
const FIRMWARE_VERSION: u8 = 2;
#[allow(dead_code)]
const ID: u8 = 3;
#[allow(dead_code)]
const BAUD_RATE: u8 = 4;
const MAX_TORQUE: u8 = 14;

// RAM table
#[allow(dead_code)]
const GOAL_POSITION: u8 = 30;
#[allow(dead_code)]
const PRESENT_POSITION: u8 = 36;
#[allow(dead_code)]
const PRESENT_TEMPERATURE: u8 = 43;

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
        // let status = self.port.read_message()?;
        // println!("{:?}", status);
        Ok(())
    }

    pub fn read_temperature(&mut self, id: u8) -> Result<u8, Box<dyn Error>> {
        Ok(self.port.read_u8(id, PRESENT_TEMPERATURE)?)
    }

    pub fn read_position(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let position = self.port.read_u16(id, PRESENT_POSITION)? as f32;
        let position = position / 3.41;
        Ok(position)
    }

    pub fn write_position(&mut self, id: u8, pos: f32) -> Result<(), Box<dyn Error>> {
        let goal_position = ((pos*3.41) as i32) as u16;
        Ok(self.port.write_u16(id, GOAL_POSITION, goal_position)?)
    }

    pub fn read_max_torque(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let position = self.port.read_u16(id, MAX_TORQUE)? as f32;
        let position = position / 3.41;
        Ok(position)
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
}
