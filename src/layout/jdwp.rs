use openidea_plugin::{Error, host::Connection};
use std::time::{Duration, Instant};

const HANDSHAKE: &[u8] = b"JDWP-Handshake";
const HEADER: usize = 11;
const MAX_PACKET: usize = 16 << 20;
const TIMEOUT: Duration = Duration::from_secs(4);

pub(super) fn capture(connection: &Connection, activity: &str) -> Result<(String, Vec<u8>), Error> {
    connection.write(HANDSHAKE)?;
    if read(connection, HANDSHAKE.len(), Instant::now() + TIMEOUT)? != HANDSHAKE {
        return Err("调试握手失败，请检查应用是否为 Debug 版本，以及是否已有调试器连接".into());
    }

    let windows = windows(&request(connection, 1, b"VULW", &[])?)?;
    let window = windows
        .iter()
        .find(|window| window.contains(activity))
        .or_else(|| windows.last())
        .ok_or("前台应用还没有可检查的窗口")?;
    let mut body = Vec::new();
    body.extend(1u32.to_be_bytes());
    let name: Vec<_> = window.encode_utf16().collect();
    body.extend((name.len() as u32).to_be_bytes());
    body.extend(name.into_iter().flat_map(u16::to_be_bytes));
    for flag in [0u32, 1, 1] {
        body.extend(flag.to_be_bytes()); // Include descendants, properties and the V2 format.
    }
    Ok((window.clone(), request(connection, 2, b"VURT", &body)?))
}

fn read(connection: &Connection, size: usize, deadline: Instant) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::with_capacity(size);
    while bytes.len() < size {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("等待 Android 调试响应超时".into());
        }
        let length = (size - bytes.len()).min(1 << 20) as u32;
        let chunk = connection.read(length, remaining.as_millis().max(1) as u32)?;
        if chunk.is_empty() {
            return Err(
                "Android 调试连接未响应。请确认已运行 Debug 应用，并停止其他调试会话".into(),
            );
        }
        bytes.extend(chunk);
    }
    Ok(bytes)
}

fn request(
    connection: &Connection,
    id: u32,
    name: &[u8; 4],
    body: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut packet = Vec::with_capacity(HEADER + 8 + body.len());
    packet.extend(((HEADER + 8 + body.len()) as u32).to_be_bytes());
    packet.extend(id.to_be_bytes());
    packet.extend([0, 199, 1]); // JDWP's DDMS command set and command.
    packet.extend(name);
    packet.extend((body.len() as u32).to_be_bytes());
    packet.extend(body);
    connection.write(&packet)?;

    let deadline = Instant::now() + TIMEOUT;
    loop {
        let header = read(connection, HEADER, deadline)?;
        let size = u32::from_be_bytes(header[0..4].try_into().unwrap()) as usize;
        if !(HEADER..=MAX_PACKET).contains(&size) {
            return Err("Android 调试响应长度无效或超过 16 MiB".into());
        }
        let mut data = read(connection, size - HEADER, deadline)?;
        if header[8] & 0x80 == 0 || header[4..8] != id.to_be_bytes() {
            continue;
        }
        let error = u16::from_be_bytes([header[9], header[10]]);
        if error != 0 {
            return Err(format!("Android 拒绝布局检查请求（JDWP {error}）").into());
        }
        if data.starts_with(b"FAIL") {
            return Err("Android 无法抓取所选窗口的布局，窗口可能已经关闭".into());
        }
        if data.len() < 8
            || !data.starts_with(name)
            || u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize != data.len() - 8
        {
            return Err("Android 返回了无效的布局检查响应".into());
        }
        return Ok(data.split_off(8));
    }
}

fn windows(mut data: &[u8]) -> Result<Vec<String>, Error> {
    fn count(data: &mut &[u8]) -> Option<usize> {
        let (prefix, rest) = data.split_at_checked(4)?;
        *data = rest;
        Some(u32::from_be_bytes(prefix.try_into().unwrap()) as usize)
    }

    let invalid = || Error::from("Android 返回了无效的窗口列表");
    let length = count(&mut data)
        .filter(|&n| n <= data.len() / 4)
        .ok_or_else(invalid)?;
    let mut windows = Vec::with_capacity(length);
    for _ in 0..length {
        let bytes = count(&mut data)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(invalid)?;
        let (name, rest) = data.split_at_checked(bytes).ok_or_else(invalid)?;
        data = rest;
        let name: Vec<_> = name
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        windows.push(String::from_utf16(&name).map_err(|_| invalid())?);
    }
    if !data.is_empty() {
        return Err(invalid());
    }
    Ok(windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_window_names() {
        assert!(windows(&[0, 0, 0, 1, 0, 0, 0, 4, 0, b'A']).is_err());
    }
}
