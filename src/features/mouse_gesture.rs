/// Mouse gesture recognition: draw shapes with mouse to trigger commands.
///
/// Recognized gestures: L (screenshot), Circle (record), Z (undo), → (next), ← (back).

#[derive(Clone, Debug, PartialEq)]
pub enum RecognizedGesture {
    L,
    Circle,
    Z,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    Unknown,
}

pub struct GestureRecognizer {
    points: Vec<(f32, f32)>,
    recording: bool,
}

impl GestureRecognizer {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            recording: false,
        }
    }

    pub fn start(&mut self, x: f32, y: f32) {
        self.points.clear();
        self.points.push((x, y));
        self.recording = true;
    }

    pub fn add_point(&mut self, x: f32, y: f32) {
        if self.recording {
            self.points.push((x, y));
        }
    }

    pub fn end(&mut self) -> RecognizedGesture {
        self.recording = false;
        if self.points.len() < 5 {
            return RecognizedGesture::Unknown;
        }
        self.recognize()
    }

    fn recognize(&self) -> RecognizedGesture {
        let dirs = self.direction_sequence();
        if dirs.is_empty() {
            return RecognizedGesture::Unknown;
        }

        // Simple direction pattern matching
        let pattern: String = dirs
            .iter()
            .map(|d| match d {
                Dir::Right => 'R',
                Dir::Left => 'L',
                Dir::Down => 'D',
                Dir::Up => 'U',
            })
            .collect();

        match pattern.as_str() {
            s if s.starts_with("DR") => RecognizedGesture::L,
            s if s.starts_with("RDL") || s.starts_with("RDLU") => RecognizedGesture::Circle,
            s if s.starts_with("RLD") || s.starts_with("RLDR") => RecognizedGesture::Z,
            s if s.chars().filter(|&c| c == 'R').count() > s.len() / 2 => {
                RecognizedGesture::ArrowRight
            }
            s if s.chars().filter(|&c| c == 'L').count() > s.len() / 2 => {
                RecognizedGesture::ArrowLeft
            }
            s if s.chars().filter(|&c| c == 'U').count() > s.len() / 2 => {
                RecognizedGesture::ArrowUp
            }
            s if s.chars().filter(|&c| c == 'D').count() > s.len() / 2 => {
                RecognizedGesture::ArrowDown
            }
            _ => RecognizedGesture::Unknown,
        }
    }

    fn direction_sequence(&self) -> Vec<Dir> {
        let step = (self.points.len() / 8).max(1);
        let sampled: Vec<_> = self.points.iter().step_by(step).collect();
        let mut dirs = Vec::new();

        for w in sampled.windows(2) {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            if dx.abs() < 5.0 && dy.abs() < 5.0 {
                continue;
            }
            let dir = if dx.abs() > dy.abs() {
                if dx > 0.0 { Dir::Right } else { Dir::Left }
            } else {
                if dy > 0.0 { Dir::Down } else { Dir::Up }
            };
            if dirs.last() != Some(&dir) {
                dirs.push(dir);
            }
        }
        dirs
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Dir {
    Right,
    Left,
    Up,
    Down,
}
