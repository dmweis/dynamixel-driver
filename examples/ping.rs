use dynamixel_driver;

fn main() {
    let mut driver = dynamixel_driver::DynamixelDriver::new("COM11").unwrap();
    for i in 1..254 {
        if driver.ping(i).is_ok() {
            println!("Found servo at {}", i);
        } else {
            println!("Not found at {}", i);
        }
    }
}