use std::io::{Cursor, Read, Seek, SeekFrom};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    errors::Error as SymphError,
    formats::FormatOptions,
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
};

struct MemSource(Cursor<Vec<u8>>);

impl Read for MemSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for MemSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl MediaSource for MemSource {
    fn is_seekable(&self) -> bool { true }
    fn byte_len(&self) -> Option<u64> { Some(self.0.get_ref().len() as u64) }
}

fn make_mss(data: &[u8]) -> MediaSourceStream {
    MediaSourceStream::new(
        Box::new(MemSource(Cursor::new(data.to_vec()))),
        Default::default(),
    )
}

// Returns (sample_rate, channel_count, duration_ms)
pub fn query(data: &[u8]) -> Result<(u32, u32, u64), String> {
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            make_mss(data),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;

    let track = probed
        .format
        .default_track()
        .ok_or("no audio track found")?;
    let p = &track.codec_params;

    let sample_rate = p.sample_rate.unwrap_or(0);
    let channels = p.channels.map(|c| c.count() as u32).unwrap_or(0);
    let n_frames = p.n_frames.unwrap_or(0);
    let duration_ms = if sample_rate > 0 {
        n_frames * 1000 / sample_rate as u64
    } else {
        0
    };

    Ok((sample_rate, channels, duration_ms))
}

// Returns (interleaved f32 samples, sample_rate, channel_count).
// The caller (lib.rs) is responsible for allocating output memory.
pub fn decode(data: &[u8]) -> Result<(Vec<f32>, u32, u32), String> {
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            make_mss(data),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or("no audio track found")?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let channels = codec_params
        .channels
        .map(|c| c.count() as u32)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;

    let mut samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphError::IoError(_)) => break,
            Err(SymphError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buf.samples());
            }
            Err(SymphError::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok((samples, sample_rate, channels))
}
