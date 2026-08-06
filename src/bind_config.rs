use std::net::SocketAddr;

pub fn resolve_bind_address(
    cli_arg: Option<&str>,
    env_var: Option<&str>,
) -> Result<SocketAddr, String> {
    match cli_arg.or(env_var) {
        Some(v) => parse_addr(v),
        None => Ok("127.0.0.1:8080".parse().unwrap()),
    }
}

fn parse_addr(v: &str) -> Result<SocketAddr, String> {
    v.parse().map_err(|_| format!("invalid bind address: {v}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_loopback_when_nothing_provided() {
        let addr = resolve_bind_address(None, None).unwrap();
        assert_eq!(addr, "127.0.0.1:8080".parse().unwrap());
    }

    #[test]
    fn uses_cli_arg_when_provided() {
        let addr = resolve_bind_address(Some("0.0.0.0:9000"), None).unwrap();
        assert_eq!(addr, "0.0.0.0:9000".parse().unwrap());
    }

    #[test]
    fn uses_env_var_when_no_cli_arg() {
        let addr = resolve_bind_address(None, Some("192.168.1.5:8080")).unwrap();
        assert_eq!(addr, "192.168.1.5:8080".parse().unwrap());
    }

    #[test]
    fn cli_arg_takes_precedence_over_env_var() {
        let addr = resolve_bind_address(Some("0.0.0.0:9000"), Some("192.168.1.5:8080")).unwrap();
        assert_eq!(addr, "0.0.0.0:9000".parse().unwrap());
    }

    #[test]
    fn rejects_invalid_cli_arg() {
        let result = resolve_bind_address(Some("not-an-address"), None);
        assert!(result.is_err());
    }
}
