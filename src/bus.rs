//! Session bus access: blocking calls plus pipelined batches, so one slow peer
//! costs a single timeout instead of one per call.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dbus::Message;
use dbus::arg::{AppendAll, PropMap};
use dbus::channel::{BusType, Channel};
use dbus::message::MessageType;

pub const TIMEOUT: Duration = Duration::from_millis(500);

pub struct Bus {
    channel: Channel,
}

impl Bus {
    pub fn session() -> Result<Bus, String> {
        Ok(Bus {
            channel: Channel::get_private(BusType::Session)
                .map_err(|e| format!("session bus: {e}"))?,
        })
    }

    pub fn call<A: AppendAll>(
        &self,
        dest: &str,
        path: &str,
        iface: &str,
        method: &str,
        args: A,
    ) -> Result<Message, String> {
        let msg = method_call(dest, path, iface, method, args)?;
        self.channel
            .send_with_reply_and_block(msg, TIMEOUT)
            .map_err(|e| format!("{dest} {method}: {}", e.message().unwrap_or("failed")))
    }

    /// Fire and forget.
    pub fn send<A: AppendAll>(&self, dest: &str, path: &str, iface: &str, method: &str, args: A) {
        if let Ok(mut msg) = method_call(dest, path, iface, method, args) {
            msg.set_no_reply(true);
            let _ = self.channel.send(msg);
            self.channel.flush();
        }
    }

    /// Sends every message at once and collects the successful replies, in order.
    /// After the first reply, the rest get `grace` more time instead of the full `cap`:
    /// live peers answer within a millisecond of each other, dead ones never do.
    pub fn call_all(
        &self,
        msgs: Vec<Message>,
        cap: Duration,
        grace: Option<Duration>,
    ) -> Vec<Option<Message>> {
        let mut replies: Vec<Option<Message>> = msgs.iter().map(|_| None).collect();
        let mut pending: HashMap<u32, usize> = HashMap::new();
        for (i, msg) in msgs.into_iter().enumerate() {
            if let Ok(serial) = self.channel.send(msg) {
                pending.insert(serial, i);
            }
        }
        let mut deadline = Instant::now() + cap;
        while !pending.is_empty() {
            let now = Instant::now();
            if now >= deadline || self.channel.read_write(Some(deadline - now)).is_err() {
                break;
            }
            while let Some(msg) = self.channel.pop_message() {
                let Some(i) = msg.get_reply_serial().and_then(|s| pending.remove(&s)) else {
                    continue;
                };
                if let Some(grace) = grace {
                    deadline = deadline.min(Instant::now() + grace);
                }
                if msg.msg_type() == MessageType::MethodReturn {
                    replies[i] = Some(msg);
                }
            }
        }
        replies
    }

    pub fn notify(&self, summary: &str, body: &str) {
        self.send(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "Notify",
            (
                "omarchy-hud",
                0u32,
                "",
                summary,
                body,
                Vec::<String>::new(),
                PropMap::new(),
                -1i32,
            ),
        );
    }
}

pub fn method_call<A: AppendAll>(
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    args: A,
) -> Result<Message, String> {
    let mut msg = Message::new_method_call(dest, path, iface, method)?;
    msg.append_all(args);
    Ok(msg)
}
