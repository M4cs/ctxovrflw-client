use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Read a JSON-RPC message from stdin (Content-Length framed)
pub async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(len_str.trim().parse()?);
        }
    }

    let length = content_length.ok_or_else(|| anyhow::anyhow!("Missing Content-Length header"))?;

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;

    Ok(Some(String::from_utf8(body)?))
}

/// Write a JSON-RPC message to stdout (Content-Length framed)
pub async fn write_message<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    message: &str,
) -> Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", message.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(message.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
