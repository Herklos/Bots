use super::*;

/// How many group-thread projections are in flight at once.
pub(crate) const READ_LIMIT: usize = 4;

/// After a dispatch, show the member working until a live item arrives, or this long.
pub(crate) const WORKING_GRACE: Duration = Duration::from_secs(2);

/// How long a failed projection keeps the last successful answer.
pub(crate) const WORKING_TTL: Duration = Duration::from_secs(5);

/// What one thread read told us. A failure is not "idle".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberProbe {
    Read { working: bool },
    Failed,
}

/// Last successful read, and the dispatch that has not shown a live item yet.
#[derive(Debug, Default)]
pub(crate) struct WorkingBook {
    pub(crate) known: HashMap<String, (bool, Instant)>,
    pub(crate) sent: HashMap<String, Instant>,
}

impl WorkingBook {
    /// The member was just dispatched. The projection may not show a run yet.
    pub(crate) fn note(&mut self, thread: &str, now: Instant) {
        if !thread.is_empty() {
            self.sent.insert(thread.to_string(), now);
        }
    }

    /// `true` when the inbox should show this thread working.
    /// A [`MemberProbe::Failed`] read keeps the last value for [`WORKING_TTL`] and does not
    /// store `false`. A dispatch stays working until the first live item or [`WORKING_GRACE`].
    pub(crate) fn decide(&mut self, thread: &str, probe: MemberProbe, now: Instant) -> bool {
        let grace = self
            .sent
            .get(thread)
            .is_some_and(|at| now.saturating_duration_since(*at) < WORKING_GRACE);
        match probe {
            MemberProbe::Read { working } => {
                if working {
                    self.sent.remove(thread);
                }
                self.known.insert(thread.to_string(), (working, now));
                working || grace
            }
            MemberProbe::Failed => {
                if grace {
                    return true;
                }
                match self.known.get(thread) {
                    Some((value, at)) if now.saturating_duration_since(*at) < WORKING_TTL => *value,
                    _ => false,
                }
            }
        }
    }
}

/// Loads group threads a few at a time. The key is the thread id.
/// A failed call is [`MemberProbe::Failed`] and carries no messages.
pub(crate) async fn read_group_threads<F, Fut>(
    thread_ids: Vec<String>,
    fetch: F,
) -> HashMap<String, (MemberProbe, Vec<Value>)>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = anyhow::Result<(bool, Vec<Value>)>>,
{
    let mut out = HashMap::new();
    let mut ids = thread_ids;
    ids.sort();
    ids.dedup();
    for chunk in ids.chunks(READ_LIMIT) {
        let loads = chunk.iter().map(|id| {
            let load = fetch(id.clone());
            let id = id.clone();
            async move { (id, load.await) }
        });
        for (id, result) in join_all(loads).await {
            match result {
                Ok((working, messages)) => {
                    out.insert(id, (MemberProbe::Read { working }, messages));
                }
                Err(_) => {
                    out.insert(id, (MemberProbe::Failed, Vec::new()));
                }
            }
        }
    }
    out
}
