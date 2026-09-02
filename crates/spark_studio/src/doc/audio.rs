//! Audio on the arrangement: the song and any other sound are files
//! (assets) placed as clips, and every audio track has a volume. The
//! song is asset [`SONG`], named by the `audio` line as it always was;
//! its clips are `sclip 0 …` lines. A file with an `audio` line and no
//! song clip gets one covering the whole file at zero — every project
//! written before audio had a `start` opens unchanged.
//!
//! Lines: `asset <id> sound <path>` (parsed beside the mesh and comp
//! assets), `sclip <asset> <start> <len> <offset>`, `volume <asset>
//! <gain>` (written only when it isn't unity).

/// The song's asset id: the one audio file that is analyzed — tempo,
/// the grid's phase and the react curves all come from it.
pub const SONG: u32 = 0;

/// Another audio file the comp plays — a voice-over, a hit, a riser —
/// never analyzed. Ids count from 1; the song is 0.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundAsset {
    pub id: u32,
    pub path: String,
}

/// One audio clip on the arrangement: `asset` plays from `start` for
/// `len` seconds of the timeline, its left edge `offset` seconds into
/// the file. A `len` of zero means *to the end of the file*: the file's
/// length isn't known until it is decoded, and a clip that plays the
/// whole thing shouldn't have to say how long that is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioClip {
    pub asset: u32,
    pub start: f32,
    pub len: f32,
    pub offset: f32,
}

impl AudioClip {
    /// The whole file, from `start`.
    pub fn whole(asset: u32, start: f32) -> Self {
        Self {
            asset,
            start: start.max(0.0),
            len: 0.0,
            offset: 0.0,
        }
    }

    /// How long it plays, given the file's length in seconds.
    pub fn span(&self, file_len: f32) -> f32 {
        if self.len > 0.0 {
            self.len
        } else {
            (file_len - self.offset).max(0.0)
        }
    }

    pub fn end(&self, file_len: f32) -> f32 {
        self.start + self.span(file_len)
    }

    /// Whether timeline time `t` is inside the clip. The start forgives
    /// the same hair an object clip does (`ObjClip::contains`).
    pub fn contains(&self, t: f32, file_len: f32) -> bool {
        t >= self.start - super::EDGE && t < self.end(file_len)
    }

    /// File time for timeline time `t` — the sample the clip plays then.
    pub fn local(&self, t: f32) -> f32 {
        t - self.start + self.offset
    }
}

/// The lines for the sounds, the clips and the volumes.
pub(super) fn write(out: &mut String, sounds: &[SoundAsset], aclips: &[AudioClip], volumes: &[(u32, f32)]) {
    for s in sounds {
        out.push_str(&format!("asset {} sound {}\n", s.id, s.path));
    }
    for c in aclips {
        out.push_str(&format!(
            "sclip {} {} {} {}\n",
            c.asset, c.start, c.len, c.offset
        ));
    }
    for (id, gain) in volumes {
        if (*gain - 1.0).abs() > 1e-6 {
            out.push_str(&format!("volume {id} {gain}\n"));
        }
    }
}

/// Read an `sclip` or `volume` line into place. False when the line is
/// neither.
pub(super) fn parse_line(line: &str, aclips: &mut Vec<AudioClip>, volumes: &mut Vec<(u32, f32)>) -> bool {
    if let Some(rest) = line.strip_prefix("sclip ") {
        let mut tok = rest.split_whitespace();
        if let (Some(Ok(asset)), Some(Ok(start)), Some(Ok(len)), Some(Ok(offset))) = (
            tok.next().map(str::parse::<u32>),
            tok.next().map(str::parse::<f32>),
            tok.next().map(str::parse::<f32>),
            tok.next().map(str::parse::<f32>),
        ) {
            aclips.push(AudioClip {
                asset,
                start: start.max(0.0),
                len: len.max(0.0),
                offset: offset.max(0.0),
            });
        }
        return true;
    }
    if let Some(rest) = line.strip_prefix("volume ") {
        let mut tok = rest.split_whitespace();
        if let (Some(Ok(id)), Some(Ok(gain))) = (
            tok.next().map(str::parse::<u32>),
            tok.next().map(str::parse::<f32>),
        ) {
            volumes.retain(|(i, _)| *i != id);
            volumes.push((id, gain.max(0.0)));
        }
        return true;
    }
    false
}

/// After the whole file is read: a song with no clip of its own plays
/// whole from zero — the way every file before `sclip` lines meant it.
pub(super) fn finish(audio: Option<&str>, aclips: &mut Vec<AudioClip>) {
    if audio.is_some() && !aclips.iter().any(|c| c.asset == SONG) {
        aclips.push(AudioClip::whole(SONG, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Doc, parse, serialize};

    /// A placed song, a trimmed sound, and a volume all survive the file;
    /// unity volumes write nothing.
    #[test]
    fn audio_clips_and_volumes_round_trip() {
        let text = serialize(&Doc {
            audio: Some("/music/drop.wav".into()),
            sounds: vec![SoundAsset {
                id: 1,
                path: "vo/intro.wav".into(),
            }],
            aclips: vec![
                AudioClip::whole(SONG, 6.857),
                AudioClip {
                    asset: 1,
                    start: 0.5,
                    len: 4.0,
                    offset: 1.25,
                },
            ],
            volumes: vec![(SONG, 1.0), (1, 0.5)],
            ..Default::default()
        });
        assert!(text.contains("asset 1 sound vo/intro.wav\n"), "{text}");
        assert!(text.contains("sclip 0 6.857 0 0\n"), "{text}");
        assert!(text.contains("volume 1 0.5\n"), "{text}");
        assert!(!text.contains("volume 0"), "unity is the default: {text}");
        let d = parse(&text);
        assert_eq!(d.sounds.len(), 1);
        assert_eq!(d.sounds[0].path, "vo/intro.wav");
        assert_eq!(d.aclips.len(), 2);
        assert_eq!(d.aclips[0], AudioClip::whole(SONG, 6.857));
        assert_eq!(d.aclips[1].asset, 1);
        assert!((d.aclips[1].offset - 1.25).abs() < 1e-6);
        assert_eq!(d.volumes, vec![(1, 0.5)]);
    }

    /// A file from before audio had a start: the song plays whole from
    /// zero, exactly as it did.
    #[test]
    fn an_old_file_places_its_song_at_zero() {
        let d = parse("spark-comp v2\naudio /music/drop.wav\n");
        assert_eq!(d.aclips, vec![AudioClip::whole(SONG, 0.0)]);
        let silent = parse("spark-comp v2\n");
        assert!(silent.aclips.is_empty(), "no song, no song clip");
    }

    /// A whole-file clip's length is the file's, less its trim; an
    /// explicit length stands on its own.
    #[test]
    fn a_clips_span_resolves_against_its_file() {
        let whole = AudioClip::whole(SONG, 2.0);
        assert_eq!(whole.span(180.0), 180.0);
        assert_eq!(whole.end(180.0), 182.0);
        let trimmed = AudioClip {
            asset: 1,
            start: 2.0,
            len: 0.0,
            offset: 30.0,
        };
        assert_eq!(trimmed.span(180.0), 150.0);
        let fixed = AudioClip {
            asset: 1,
            start: 2.0,
            len: 4.0,
            offset: 30.0,
        };
        assert_eq!(fixed.span(180.0), 4.0);
        assert!(fixed.contains(2.0, 180.0) && fixed.contains(5.99, 180.0));
        assert!(!fixed.contains(6.0, 180.0) && !fixed.contains(1.9, 180.0));
        assert!((fixed.local(3.0) - 31.0).abs() < 1e-6, "file time through the trim");
    }
}
