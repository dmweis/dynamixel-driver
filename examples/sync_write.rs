use dynamixel_driver::{DynamixelDriver, SyncCommand};
use std::{time::Duration, thread::sleep};
fn main() {
    let mut driver = DynamixelDriver::new("COM11").unwrap();
    let commands = vec![
        SyncCommand::new(5, 1023),
        SyncCommand::new(9, 1023),
    ];
    driver.sync_write_position(commands).unwrap();
    sleep(Duration::from_secs(2));
    let commands = vec![
        SyncCommand::new(5, 0),
        SyncCommand::new(9, 0),
    ];
    driver.sync_write_position(commands).unwrap();
}