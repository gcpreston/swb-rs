use url::Url;

struct SpectatorModeClient {

}

impl SpectatorModeClient {
  async fn run(&self) -> Result<(), Error> {

  }
}

struct ClientConfig {
  url: Url
}

pub fn initiate_connection(config: ClientConfig) -> SpectatorModeClient {

}


fn parse_connect_reply(reply: Message) -> Result<(String, String), &'static str> {
    match reply {
        Message::Text(bytes) => {
            let v: Value = serde_json::from_str(bytes.as_str()).unwrap();
            if let Value::String(bridge_id) = &v["bridge_id"] {
                if let Value::String(reconnect_token) = &v["reconnect_token"] {
                    Ok((bridge_id.to_string(), reconnect_token.to_string()))
                } else {
                    Err("where's reconnect token?")
                }
            } else {
                Err("where's bridge id?")
            }
        }
        _ => Err("didn't expect that!"),
    }
}
