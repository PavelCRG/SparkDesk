use std::io::Cursor;

use image::{ColorType, ImageFormat, ImageReader, codecs::jpeg::JpegEncoder, imageops::FilterType};
use xcap::Monitor;

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_WIDTH: u32 = 1280;
const MAX_HEIGHT: u32 = 720;
const CHUNK_BYTES: usize = 16_000;
const HEADER_BYTES: usize = 19;
const SCREEN_CAPTURE_BACKLOG_BYTES: usize = 256 * 1024;
pub const SCREEN_SEND_BUFFER_BYTES: usize = 512 * 1024;
pub const SCREEN_FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_micros(16_667);

pub fn can_capture_screen_frame(outstanding_bytes: usize) -> bool {
    outstanding_bytes < SCREEN_CAPTURE_BACKLOG_BYTES
}

#[derive(Debug, PartialEq)]
pub struct ScreenFrame {
    pub id: u32,
    pub width: u16,
    pub height: u16,
    pub jpeg: Vec<u8>,
}

pub fn validate_jpeg(frame: &ScreenFrame) -> bool {
    ImageReader::with_format(Cursor::new(&frame.jpeg), ImageFormat::Jpeg)
        .into_dimensions()
        .is_ok_and(|(width, height)| {
            width == u32::from(frame.width)
                && height == u32::from(frame.height)
                && width <= MAX_WIDTH
                && height <= MAX_HEIGHT
        })
}

pub fn primary_monitor() -> Result<Monitor, String> {
    Monitor::all()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .ok_or_else(|| "no primary monitor was found".to_owned())
}

pub fn capture_primary() -> Result<ScreenFrame, String> {
    let monitor = primary_monitor()?;
    let mut image = monitor.capture_image().map_err(|error| error.to_string())?;
    let scale = (MAX_WIDTH as f64 / image.width() as f64)
        .min(MAX_HEIGHT as f64 / image.height() as f64)
        .min(1.0);
    if scale < 1.0 {
        image = image::imageops::resize(
            &image,
            (image.width() as f64 * scale).round() as u32,
            (image.height() as f64 * scale).round() as u32,
            FilterType::Triangle,
        );
    }
    let mut image = image::DynamicImage::ImageRgba8(image).to_rgb8();

    loop {
        let mut jpeg = Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut jpeg, 45)
            .encode(
                image.as_raw(),
                image.width(),
                image.height(),
                ColorType::Rgb8.into(),
            )
            .map_err(|error| error.to_string())?;
        if jpeg.get_ref().len() <= MAX_FRAME_BYTES {
            return Ok(ScreenFrame {
                id: 0,
                width: image.width() as u16,
                height: image.height() as u16,
                jpeg: jpeg.into_inner(),
            });
        }
        if image.width() <= 320 || image.height() <= 180 {
            return Err("screen frame exceeds the 1 MiB transport limit".into());
        }
        image = image::imageops::resize(
            &image,
            image.width() / 2,
            image.height() / 2,
            FilterType::Triangle,
        );
    }
}

fn frame_wire_size(frame: &ScreenFrame) -> Result<usize, &'static str> {
    if frame.width == 0
        || frame.height == 0
        || u32::from(frame.width) > MAX_WIDTH
        || u32::from(frame.height) > MAX_HEIGHT
        || frame.jpeg.is_empty()
        || frame.jpeg.len() > MAX_FRAME_BYTES
    {
        return Err("invalid screen frame");
    }
    let count = frame.jpeg.len().div_ceil(CHUNK_BYTES);
    u16::try_from(count).map_err(|_| "too many screen chunks")?;
    Ok(frame.jpeg.len() + count * HEADER_BYTES)
}

pub fn chunk_frame_iter(
    frame: &ScreenFrame,
) -> Result<impl Iterator<Item = Vec<u8>> + '_, &'static str> {
    frame_wire_size(frame)?;
    let count = u16::try_from(frame.jpeg.len().div_ceil(CHUNK_BYTES))
        .map_err(|_| "too many screen chunks")?;
    Ok(frame
        .jpeg
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(move |(index, bytes)| {
            let mut chunk = Vec::with_capacity(HEADER_BYTES + bytes.len());
            chunk.extend_from_slice(b"SD");
            chunk.push(1);
            chunk.extend_from_slice(&frame.id.to_be_bytes());
            chunk.extend_from_slice(&frame.width.to_be_bytes());
            chunk.extend_from_slice(&frame.height.to_be_bytes());
            chunk.extend_from_slice(&(index as u16).to_be_bytes());
            chunk.extend_from_slice(&count.to_be_bytes());
            chunk.extend_from_slice(&(frame.jpeg.len() as u32).to_be_bytes());
            chunk.extend_from_slice(bytes);
            chunk
        }))
}

#[cfg(test)]
fn chunk_frame(frame: &ScreenFrame) -> Result<Vec<Vec<u8>>, &'static str> {
    Ok(chunk_frame_iter(frame)?.collect())
}

#[derive(Default)]
pub struct FrameAssembler {
    current: Option<PartialFrame>,
    last_completed_id: Option<u32>,
}

fn is_newer_frame_id(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < (1 << 31)
}

struct PartialFrame {
    id: u32,
    width: u16,
    height: u16,
    total_bytes: usize,
    chunks: Vec<Option<Vec<u8>>>,
    received_bytes: usize,
}

impl FrameAssembler {
    pub fn push(&mut self, data: &[u8]) -> Result<Option<ScreenFrame>, &'static str> {
        if data.len() < HEADER_BYTES
            || data.len() > HEADER_BYTES + CHUNK_BYTES
            || &data[..2] != b"SD"
            || data[2] != 1
        {
            return Err("invalid screen chunk");
        }
        let id = u32::from_be_bytes(data[3..7].try_into().unwrap());
        let width = u16::from_be_bytes(data[7..9].try_into().unwrap());
        let height = u16::from_be_bytes(data[9..11].try_into().unwrap());
        let index = usize::from(u16::from_be_bytes(data[11..13].try_into().unwrap()));
        let count = usize::from(u16::from_be_bytes(data[13..15].try_into().unwrap()));
        let total_bytes = u32::from_be_bytes(data[15..19].try_into().unwrap()) as usize;
        if width == 0
            || height == 0
            || u32::from(width) > MAX_WIDTH
            || u32::from(height) > MAX_HEIGHT
            || total_bytes == 0
            || total_bytes > MAX_FRAME_BYTES
            || count == 0
            || count != total_bytes.div_ceil(CHUNK_BYTES)
            || index >= count
        {
            return Err("invalid screen chunk metadata");
        }
        let payload = &data[HEADER_BYTES..];
        let expected_len = if index + 1 == count {
            total_bytes - index * CHUNK_BYTES
        } else {
            CHUNK_BYTES
        };
        if payload.len() != expected_len {
            return Err("invalid screen chunk size");
        }
        if self
            .last_completed_id
            .is_some_and(|last_id| !is_newer_frame_id(id, last_id))
        {
            return Err("stale screen frame");
        }
        if let Some(current) = &self.current
            && current.id != id
            && !is_newer_frame_id(id, current.id)
        {
            return Err("stale screen frame");
        }
        if self.current.as_ref().is_none_or(|frame| frame.id != id) {
            self.current = Some(PartialFrame {
                id,
                width,
                height,
                total_bytes,
                chunks: std::iter::repeat_with(|| None).take(count).collect(),
                received_bytes: 0,
            });
        }
        let frame = self.current.as_mut().ok_or("screen frame state missing")?;
        if frame.width != width || frame.height != height || frame.total_bytes != total_bytes {
            self.current = None;
            return Err("inconsistent screen frame metadata");
        }
        if frame.chunks[index].is_none() {
            frame.received_bytes += payload.len();
            frame.chunks[index] = Some(payload.to_vec());
        }
        if frame.received_bytes != frame.total_bytes {
            return Ok(None);
        }
        let mut jpeg = Vec::with_capacity(frame.total_bytes);
        for chunk in frame.chunks.iter().flatten() {
            jpeg.extend_from_slice(chunk);
        }
        self.last_completed_id = Some(id);
        self.current.take().ok_or("screen frame state missing")?;
        Ok(Some(ScreenFrame {
            id,
            width,
            height,
            jpeg,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FrameAssembler, SCREEN_FRAME_INTERVAL, ScreenFrame, can_capture_screen_frame, chunk_frame,
        chunk_frame_iter, frame_wire_size, validate_jpeg,
    };
    use image::{ColorType, codecs::jpeg::JpegEncoder};
    use std::io::Cursor;

    #[test]
    fn chunks_and_reassembles_a_frame_larger_than_one_data_channel_message() {
        let frame = ScreenFrame {
            id: 42,
            width: 1280,
            height: 720,
            jpeg: vec![9; 35_000],
        };
        let chunks = chunk_frame(&frame).unwrap();
        assert!(chunks.iter().all(|chunk| chunk.len() <= 16_384));

        let mut assembler = FrameAssembler::default();
        let mut result = None;
        for chunk in chunks {
            result = assembler.push(&chunk).unwrap().or(result);
        }
        assert_eq!(result, Some(frame));
    }

    #[test]
    fn rejects_oversized_or_invalid_screen_frames() {
        let frame = ScreenFrame {
            id: 1,
            width: 1921,
            height: 1080,
            jpeg: vec![1],
        };
        assert!(chunk_frame(&frame).is_err());
        assert!(FrameAssembler::default().push(b"bad").is_err());
    }

    #[test]
    fn validates_jpeg_data_and_dimensions_before_display() {
        let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([30, 50, 70, 255]));
        let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
        let mut jpeg = Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut jpeg, 55)
            .encode(rgb.as_raw(), 2, 1, ColorType::Rgb8.into())
            .unwrap();
        let frame = ScreenFrame {
            id: 1,
            width: 2,
            height: 1,
            jpeg: jpeg.into_inner(),
        };
        assert!(validate_jpeg(&frame));
        assert!(!validate_jpeg(&ScreenFrame { width: 3, ..frame }));
    }

    #[test]
    fn capture_period_targets_sixty_frames_per_second() {
        assert_eq!(SCREEN_FRAME_INTERVAL.as_micros(), 16_667);
    }

    #[test]
    fn screen_capture_pauses_when_the_send_queue_is_backlogged() {
        assert!(can_capture_screen_frame(0));
        assert!(can_capture_screen_frame(128 * 1024));
        assert!(!can_capture_screen_frame(256 * 1024));
    }

    #[test]
    fn lazy_screen_chunks_match_the_existing_wire_format() {
        let frame = ScreenFrame {
            id: 9,
            width: 1280,
            height: 720,
            jpeg: vec![7; 35_000],
        };

        let lazy = chunk_frame_iter(&frame).unwrap().collect::<Vec<_>>();
        assert_eq!(lazy, chunk_frame(&frame).unwrap());
        assert_eq!(
            frame_wire_size(&frame).unwrap(),
            lazy.iter().map(Vec::len).sum::<usize>()
        );
    }

    #[test]
    fn delayed_chunks_from_older_frames_do_not_replace_newer_frames() {
        let newer = ScreenFrame {
            id: 2,
            width: 1280,
            height: 720,
            jpeg: vec![2; 35_000],
        };
        let older = ScreenFrame {
            id: 1,
            width: 1280,
            height: 720,
            jpeg: vec![1; 35_000],
        };
        let newer_chunks = chunk_frame(&newer).unwrap();
        let older_chunks = chunk_frame(&older).unwrap();
        let mut assembler = FrameAssembler::default();

        let mut result = None;
        for chunk in &newer_chunks {
            result = assembler.push(chunk).unwrap().or(result);
        }
        assert_eq!(result, Some(newer));
        assert!(assembler.push(&older_chunks[0]).is_err());
    }

    #[test]
    fn frame_id_order_handles_wraparound() {
        let last = ScreenFrame {
            id: u32::MAX,
            width: 1280,
            height: 720,
            jpeg: vec![1; 35_000],
        };
        let first = ScreenFrame {
            id: 0,
            width: 1280,
            height: 720,
            jpeg: vec![2; 35_000],
        };
        let mut assembler = FrameAssembler::default();

        for chunk in chunk_frame(&last).unwrap() {
            assembler.push(&chunk).unwrap();
        }
        let mut result = None;
        for chunk in chunk_frame(&first).unwrap() {
            result = assembler.push(&chunk).unwrap().or(result);
        }
        assert_eq!(result, Some(first));
    }
}
