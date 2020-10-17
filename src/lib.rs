mod instructions;
mod serial_driver;

use instructions::{
    Ping, ReadInstruction, SyncCommand, SyncCommandFloat, SyncWrite, WriteInstruction,
};
use serial_driver::{FramedDriver, FramedSerialDriver};

use std::error::Error;

// EEPROM table
const MODEL_NUMBER: u8 = 0;
const FIRMWARE_VERSION: u8 = 2;
const ID: u8 = 3;
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

pub struct DynamixelDriver {
    port: Box<dyn FramedDriver>,
}

impl DynamixelDriver {
    pub fn new(port_name: &str) -> Result<DynamixelDriver, Box<dyn Error>> {
        let driver = FramedSerialDriver::new(port_name)?;
        Ok(DynamixelDriver {
            port: Box::new(driver),
        })
    }

    pub fn with_baud_rate(port: &str, baud_rate: u32) -> Result<DynamixelDriver, Box<dyn Error>> {
        let driver = FramedSerialDriver::with_baud_rate(port, baud_rate)?;
        Ok(DynamixelDriver {
            port: Box::new(driver),
        })
    }

    fn with_driver(connection: Box<dyn FramedDriver>) -> DynamixelDriver {
        DynamixelDriver { port: connection }
    }

    async fn read_u8(&mut self, id: u8, addr: u8) -> Result<u8, Box<dyn Error>> {
        let command = ReadInstruction::new(id, addr, 1);
        self.port.send(Box::new(command)).await?;
        let response = self.port.receive().await?;
        Ok(response.param(0).unwrap())
    }

    async fn read_u16(&mut self, id: u8, addr: u8) -> Result<u16, Box<dyn Error>> {
        let command = ReadInstruction::new(id, addr, 2);
        self.port.send(Box::new(command)).await?;
        let response = self.port.receive().await?;
        let mut res = 0_u16;
        let a = response.param(0).unwrap() as u16;
        let b = response.param(1).unwrap() as u16;
        res |= b << 8;
        res |= a;
        Ok(res)
    }

    async fn write_u8(&mut self, id: u8, addr: u8, value: u8) -> Result<(), Box<dyn Error>> {
        let msg = WriteInstruction::with_u8(id, addr, value);
        self.port.send(Box::new(msg)).await?;
        let _response = self.port.receive().await?;
        Ok(())
    }

    async fn write_u16(&mut self, id: u8, addr: u8, value: u16) -> Result<(), Box<dyn Error>> {
        let msg = WriteInstruction::with_u16(id, addr, value);
        self.port.send(Box::new(msg)).await?;
        let _response = self.port.receive().await?;
        Ok(())
    }

    pub async fn ping(&mut self, id: u8) -> Result<(), Box<dyn Error>> {
        let ping = Ping::new(id);
        self.port.send(Box::new(ping)).await?;
        let _status = self.port.receive().await?;
        Ok(())
    }

    pub async fn write_id(&mut self, id: u8, new_id: u8) -> Result<(), Box<dyn Error>> {
        self.write_u8(id, ID, new_id).await?;
        Ok(())
    }

    pub async fn write_torque(
        &mut self,
        id: u8,
        torque_enabled: bool,
    ) -> Result<(), Box<dyn Error>> {
        if torque_enabled {
            Ok(self.write_u8(id, TORQUE_ENABLED, 1).await?)
        } else {
            Ok(self.write_u8(id, TORQUE_ENABLED, 0).await?)
        }
    }

    pub async fn read_temperature(&mut self, id: u8) -> Result<u8, Box<dyn Error>> {
        Ok(self.read_u8(id, PRESENT_TEMPERATURE).await?)
    }

    pub async fn read_voltage(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        Ok(self.read_u8(id, PRESENT_VOLTAGE).await? as f32 / 10.0)
    }

    pub async fn read_position(&mut self, id: u8) -> Result<u16, Box<dyn Error>> {
        let position = self.read_u16(id, PRESENT_POSITION).await?;
        Ok(position)
    }

    pub async fn read_position_degrees(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let position = self.read_u16(id, PRESENT_POSITION).await? as f32;
        let position = position / 3.41;
        Ok(position)
    }

    pub async fn read_position_rad(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let pos_rad = self.read_position_degrees(id).await?.to_radians();
        Ok(pos_rad)
    }

    pub async fn write_compliance_slope_both(
        &mut self,
        id: u8,
        compliance: u8,
    ) -> Result<(), Box<dyn Error>> {
        self.write_u8(id, CW_COMPLIANCE_SLOPE, compliance).await?;
        self.write_u8(id, CWW_COMPLIANCE_SLOPE, compliance).await?;
        Ok(())
    }

    pub async fn sync_write_compliance_both<T: Into<SyncCommand>>(
        &mut self,
        compliance: Vec<T>,
    ) -> Result<(), Box<dyn Error>> {
        let compliance: Vec<SyncCommand> = compliance
            .into_iter()
            .map(|command| command.into())
            .collect();
        let message_cw = SyncWrite::new(CW_COMPLIANCE_SLOPE, 1, compliance.clone());
        let message_cww = SyncWrite::new(CWW_COMPLIANCE_SLOPE, 1, compliance);
        self.port.send(Box::new(message_cw)).await?;
        self.port.send(Box::new(message_cww)).await?;
        Ok(())
    }

    pub async fn sync_write_torque<T: Into<SyncCommand>>(
        &mut self,
        torque: Vec<T>,
    ) -> Result<(), Box<dyn Error>> {
        let torque_commands: Vec<SyncCommand> =
            torque.into_iter().map(|command| command.into()).collect();
        let torque_message = SyncWrite::new(TORQUE_ENABLED, 1, torque_commands);
        self.port.send(Box::new(torque_message)).await?;
        Ok(())
    }

    pub async fn write_position(&mut self, id: u8, pos: u16) -> Result<(), Box<dyn Error>> {
        self.write_u16(id, GOAL_POSITION, pos).await?;
        Ok(())
    }

    pub async fn write_position_degrees(&mut self, id: u8, pos: f32) -> Result<(), Box<dyn Error>> {
        let goal_position = ((pos * 3.41) as i32) as u16;
        self.write_u16(id, GOAL_POSITION, goal_position).await?;
        Ok(())
    }

    pub async fn write_position_rad(&mut self, id: u8, pos: f32) -> Result<(), Box<dyn Error>> {
        self.write_position_degrees(id, pos.to_degrees()).await?;
        Ok(())
    }

    pub async fn sync_write_position<T: Into<SyncCommand>>(
        &mut self,
        positions: Vec<T>,
    ) -> Result<(), Box<dyn Error>> {
        let positions: Vec<SyncCommand> = positions
            .into_iter()
            .map(|command| command.into())
            .collect();
        let message = SyncWrite::new(GOAL_POSITION, 2, positions);
        self.port.send(Box::new(message)).await?;
        Ok(())
    }

    pub async fn sync_write_position_degrees(
        &mut self,
        positions: Vec<SyncCommandFloat>,
    ) -> Result<(), Box<dyn Error>> {
        let positions_dyn_units: Vec<SyncCommand> = positions
            .into_iter()
            .map(|command| {
                let goal_position = ((command.value() * 3.41) as i32) as u32;
                SyncCommand::new(command.id(), goal_position)
            })
            .collect();
        let message = SyncWrite::new(GOAL_POSITION, 2, positions_dyn_units);
        self.port.send(Box::new(message)).await?;
        Ok(())
    }

    pub async fn sync_write_position_rad(
        &mut self,
        positions: Vec<SyncCommandFloat>,
    ) -> Result<(), Box<dyn Error>> {
        let positions_degrees: Vec<SyncCommandFloat> = positions
            .into_iter()
            .map(|command| SyncCommandFloat::new(command.id(), command.value().to_degrees()))
            .collect();
        self.sync_write_position_degrees(positions_degrees).await?;
        Ok(())
    }

    pub async fn sync_write_moving_speed<T: Into<SyncCommand>>(
        &mut self,
        speeds: Vec<T>,
    ) -> Result<(), Box<dyn Error>> {
        let speeds: Vec<SyncCommand> = speeds.into_iter().map(|command| command.into()).collect();
        let message = SyncWrite::new(MOVING_SPEED, 2, speeds);
        self.port.send(Box::new(message)).await?;
        Ok(())
    }

    pub async fn read_max_torque(&mut self, id: u8) -> Result<f32, Box<dyn Error>> {
        let max_torque = self.read_u16(id, MAX_TORQUE).await? as f32;
        let max_torque_percentage = max_torque / 2013.0;
        Ok(max_torque_percentage)
    }

    pub async fn search_all(&mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut ids = vec![];
        for i in 1..254 {
            if self.ping(i).await.is_ok() {
                ids.push(i);
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    //     struct MockSerialPort {
    //         written_data: Sender<Vec<u8>>,
    //         mock_read_data: Vec<Status>
    //     }

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
