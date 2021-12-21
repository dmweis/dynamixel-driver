use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt()]
pub struct Args {
    #[structopt(about = "Serial port to use")]
    pub port: String,
}
