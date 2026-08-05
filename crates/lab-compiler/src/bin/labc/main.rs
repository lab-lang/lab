mod args;
mod driver;

fn main() -> anyhow::Result<()> {
    driver::run()
}
