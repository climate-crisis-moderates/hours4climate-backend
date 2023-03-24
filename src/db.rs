use crate::config::Config;

pub fn connect(config: &Config) -> Result<redis::Client, String> {
    let connection_string = format!(
        "redis://{}:{}/{}",
        config.redis_host_name, config.redis_port, config.redis_db
    );

    Ok(redis::Client::open(connection_string).unwrap())
}
