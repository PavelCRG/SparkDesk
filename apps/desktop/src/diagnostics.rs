use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

const SAMPLE_LIMIT: usize = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerConnectionStatus {
    #[default]
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(Default)]
pub struct SessionDiagnostics(Mutex<Counters>);

#[derive(Default)]
struct Counters {
    frames_captured: u64,
    bytes_captured: u64,
    last_captured_frame_id: Option<u32>,
    frames_sent: u64,
    bytes_sent: u64,
    last_sent_frame_id: Option<u32>,
    frames_received: u64,
    bytes_received: u64,
    last_received_frame_id: Option<u32>,
    frames_rendered: u64,
    last_rendered_frame_id: Option<u32>,
    backlog_skips: u64,
    incomplete_frames_dropped: u64,
    stale_chunks_dropped: u64,
    inputs_sent: u64,
    inputs_dispatched: u64,
    inputs_rejected: u64,
    input_acks_sent: u64,
    input_acks_received: u64,
    peer_state: PeerConnectionStatus,
    timings: [VecDeque<u64>; 6],
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshot {
    pub frames_captured: u64,
    pub bytes_captured: u64,
    pub last_captured_frame_id: Option<u32>,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub last_sent_frame_id: Option<u32>,
    pub frames_received: u64,
    pub bytes_received: u64,
    pub last_received_frame_id: Option<u32>,
    pub frames_rendered: u64,
    pub last_rendered_frame_id: Option<u32>,
    pub backlog_skips: u64,
    pub incomplete_frames_dropped: u64,
    pub stale_chunks_dropped: u64,
    pub inputs_sent: u64,
    pub inputs_dispatched: u64,
    pub inputs_rejected: u64,
    pub input_acks_sent: u64,
    pub input_acks_received: u64,
    pub capture_samples_retained: usize,
    pub capture_p50_us: Option<u64>,
    pub capture_p95_us: Option<u64>,
    pub resize_p50_us: Option<u64>,
    pub resize_p95_us: Option<u64>,
    pub encode_p50_us: Option<u64>,
    pub encode_p95_us: Option<u64>,
    pub send_p50_us: Option<u64>,
    pub send_p95_us: Option<u64>,
    pub reassembly_p50_us: Option<u64>,
    pub reassembly_p95_us: Option<u64>,
    pub event_p50_us: Option<u64>,
    pub event_p95_us: Option<u64>,
    pub peer_state: PeerConnectionStatus,
}

impl SessionDiagnostics {
    pub fn capture(&self, id: u32, bytes: usize, capture_us: u64, resize_us: u64, encode_us: u64) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.frames_captured += 1;
        state.bytes_captured += bytes as u64;
        state.last_captured_frame_id = Some(id);
        for (slot, sample) in [capture_us, resize_us, encode_us].into_iter().enumerate() {
            push_sample(&mut state.timings[slot], sample);
        }
    }

    pub fn send_duration(&self, elapsed_us: u64) {
        self.mutate(|state| push_sample(&mut state.timings[3], elapsed_us));
    }

    pub fn reassembly_duration(&self, elapsed_us: u64) {
        self.mutate(|state| push_sample(&mut state.timings[4], elapsed_us));
    }

    pub fn event_duration(&self, elapsed_us: u64) {
        self.mutate(|state| push_sample(&mut state.timings[5], elapsed_us));
    }

    pub fn sent(&self, id: u32, bytes: usize) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.frames_sent += 1;
        state.bytes_sent += bytes as u64;
        state.last_sent_frame_id = Some(id);
    }

    pub fn received(&self, id: u32, bytes: usize) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.frames_received += 1;
        state.bytes_received += bytes as u64;
        state.last_received_frame_id = Some(id);
    }

    pub fn rendered_batch(&self, count: u64, last_id: u32) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.frames_rendered += count;
        state.last_rendered_frame_id = Some(last_id);
    }

    pub fn skipped_backlog(&self) {
        self.mutate(|state| state.backlog_skips += 1);
    }

    pub fn dropped_incomplete_frame(&self) {
        self.mutate(|state| state.incomplete_frames_dropped += 1);
    }

    pub fn dropped_stale_chunk(&self) {
        self.mutate(|state| state.stale_chunks_dropped += 1);
    }

    pub fn input_sent(&self) {
        self.mutate(|state| state.inputs_sent += 1);
    }

    pub fn input_dispatched(&self, accepted: bool) {
        self.mutate(|state| {
            if accepted {
                state.inputs_dispatched += 1;
            } else {
                state.inputs_rejected += 1;
            }
        });
    }

    pub fn input_ack_sent(&self) {
        self.mutate(|state| state.input_acks_sent += 1);
    }

    pub fn input_ack_received(&self, applied: bool) {
        self.mutate(|state| {
            state.input_acks_received += 1;
            if applied {
                state.inputs_dispatched += 1;
            } else {
                state.inputs_rejected += 1;
            }
        });
    }

    pub fn set_connection_status(&self, status: PeerConnectionStatus) {
        self.mutate(|state| state.peer_state = status);
    }

    pub fn snapshot(&self) -> DiagnosticsSnapshot {
        let state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let percentile = |slot: usize, percentile: f64| {
            if state.timings[slot].is_empty() {
                return None;
            }
            let mut samples = state.timings[slot].iter().copied().collect::<Vec<_>>();
            samples.sort_unstable();
            Some(samples[((samples.len() as f64 * percentile).ceil() as usize).saturating_sub(1)])
        };
        DiagnosticsSnapshot {
            frames_captured: state.frames_captured,
            bytes_captured: state.bytes_captured,
            last_captured_frame_id: state.last_captured_frame_id,
            frames_sent: state.frames_sent,
            bytes_sent: state.bytes_sent,
            last_sent_frame_id: state.last_sent_frame_id,
            frames_received: state.frames_received,
            bytes_received: state.bytes_received,
            last_received_frame_id: state.last_received_frame_id,
            frames_rendered: state.frames_rendered,
            last_rendered_frame_id: state.last_rendered_frame_id,
            backlog_skips: state.backlog_skips,
            incomplete_frames_dropped: state.incomplete_frames_dropped,
            stale_chunks_dropped: state.stale_chunks_dropped,
            inputs_sent: state.inputs_sent,
            inputs_dispatched: state.inputs_dispatched,
            inputs_rejected: state.inputs_rejected,
            input_acks_sent: state.input_acks_sent,
            input_acks_received: state.input_acks_received,
            capture_samples_retained: state.timings[0].len(),
            capture_p50_us: percentile(0, 0.50),
            capture_p95_us: percentile(0, 0.95),
            resize_p50_us: percentile(1, 0.50),
            resize_p95_us: percentile(1, 0.95),
            encode_p50_us: percentile(2, 0.50),
            encode_p95_us: percentile(2, 0.95),
            send_p50_us: percentile(3, 0.50),
            send_p95_us: percentile(3, 0.95),
            reassembly_p50_us: percentile(4, 0.50),
            reassembly_p95_us: percentile(4, 0.95),
            event_p50_us: percentile(5, 0.50),
            event_p95_us: percentile(5, 0.95),
            peer_state: state.peer_state,
        }
    }

    fn mutate(&self, update: impl FnOnce(&mut Counters)) {
        update(
            &mut self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }
}

fn push_sample(samples: &mut VecDeque<u64>, sample: u64) {
    samples.push_back(sample);
    if samples.len() > SAMPLE_LIMIT {
        samples.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::{PeerConnectionStatus, SessionDiagnostics};

    #[test]
    fn session_diagnostics_bound_samples_and_track_frame_and_input_counts() {
        let diagnostics = SessionDiagnostics::default();
        for id in 0..200 {
            diagnostics.capture(id, 1024, u64::from(id), 10, 20);
        }
        diagnostics.sent(199, 1024);
        diagnostics.received(199, 1024);
        diagnostics.rendered_batch(1, 199);
        diagnostics.rendered_batch(2, 198);
        diagnostics.skipped_backlog();
        diagnostics.dropped_incomplete_frame();
        diagnostics.dropped_stale_chunk();
        diagnostics.input_sent();
        diagnostics.input_dispatched(true);
        diagnostics.input_dispatched(false);
        diagnostics.input_ack_sent();
        diagnostics.input_ack_received(true);
        diagnostics.send_duration(30);
        diagnostics.reassembly_duration(40);
        diagnostics.event_duration(50);

        diagnostics.set_connection_status(PeerConnectionStatus::Connected);
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.frames_captured, 200);
        assert_eq!(snapshot.capture_samples_retained, 120);
        assert_eq!(snapshot.frames_sent, 1);
        assert_eq!(snapshot.frames_received, 1);
        assert_eq!(snapshot.frames_rendered, 3);
        assert_eq!(snapshot.last_rendered_frame_id, Some(198));
        assert_eq!(snapshot.backlog_skips, 1);
        assert_eq!(snapshot.incomplete_frames_dropped, 1);
        assert_eq!(snapshot.stale_chunks_dropped, 1);
        assert_eq!(snapshot.inputs_sent, 1);
        assert_eq!(snapshot.inputs_dispatched, 2);
        assert_eq!(snapshot.inputs_rejected, 1);
        assert_eq!(snapshot.input_acks_sent, 1);
        assert_eq!(snapshot.input_acks_received, 1);
        assert_eq!(snapshot.capture_p50_us, Some(139));
        assert_eq!(snapshot.capture_p95_us, Some(193));
        assert_eq!(snapshot.resize_p95_us, Some(10));
        assert_eq!(snapshot.encode_p95_us, Some(20));
        assert_eq!(snapshot.send_p50_us, Some(30));
        assert_eq!(snapshot.reassembly_p95_us, Some(40));
        assert_eq!(snapshot.event_p95_us, Some(50));
        assert_eq!(snapshot.peer_state, PeerConnectionStatus::Connected);
    }

    #[test]
    fn connection_status_reports_transient_disconnects_and_terminal_failure() {
        let diagnostics = SessionDiagnostics::default();
        assert_eq!(
            diagnostics.snapshot().peer_state,
            PeerConnectionStatus::Connecting
        );
        diagnostics.set_connection_status(PeerConnectionStatus::Disconnected);
        assert_eq!(
            diagnostics.snapshot().peer_state,
            PeerConnectionStatus::Disconnected
        );
        diagnostics.set_connection_status(PeerConnectionStatus::Failed);
        assert_eq!(
            diagnostics.snapshot().peer_state,
            PeerConnectionStatus::Failed
        );
    }

    #[test]
    fn viewer_ack_records_whether_the_host_applied_remote_input() {
        let diagnostics = SessionDiagnostics::default();
        diagnostics.input_ack_received(false);
        diagnostics.input_ack_received(true);

        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.input_acks_received, 2);
        assert_eq!(snapshot.inputs_dispatched, 1);
        assert_eq!(snapshot.inputs_rejected, 1);
    }
}
