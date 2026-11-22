//! Every file job of this launch, kept: so that any file made since Taylor was opened can be checked
//! against its own receipt when it is shown, not only the last one. Bounded like the kernel's own
//! history of a run; nothing is ever dropped to make room, and a full set says so before asking.
use crate::file_job::{FileJob, Outcome};
use crate::project_location::FileDestination;

/// As many as the kernel keeps receipts for in one run of a program.
pub const KEPT: usize = 64;
/// Said of requests, not of files: a kept job may have been declined or have no known result.
pub const FULL: &str = "Limit of note requests for one opening reached. Leave and open Taylor again.";

pub struct Jobs {
    kept: [Option<FileJob>; KEPT],
    count: usize,
}

impl Jobs {
    pub const fn new() -> Self {
        Self { kept: [const { None }; KEPT], count: 0 }
    }

    pub fn len(&self) -> usize { self.count }
    pub fn is_empty(&self) -> bool { self.count == 0 }
    pub fn is_full(&self) -> bool { self.count == KEPT }

    /// Keep a job whatever became of it: made, refused, or unknown. Handed back if there is no room,
    /// which the caller must have checked before the kernel was asked.
    pub fn keep(&mut self, job: FileJob) -> Result<(), FileJob> {
        if self.is_full() {
            return Err(job);
        }
        self.kept[self.count] = Some(job);
        self.count += 1;
        Ok(())
    }

    /// The job most recently kept: what the Work panel reports.
    pub fn last(&self) -> Option<&FileJob> {
        self.count.checked_sub(1).and_then(|i| self.kept[i].as_ref())
    }

    /// The job whose receipt says this exact file was made in this launch. Create-only names mean there
    /// is at most one; a refused or unknown job for the same name is not it.
    pub fn made(&mut self, destination: &FileDestination) -> Option<&mut FileJob> {
        self.kept[..self.count].iter_mut().flatten()
            .find(|job| job.destination() == destination && job.receipt().is_some_and(|r| r.outcome == Outcome::Created))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note;

    fn receipt(p: &note::Prepared, outcome: u32, attempt: u64) -> [u8; 100] {
        let name_len = u32::from_le_bytes(p.record[24..28].try_into().unwrap()) as usize;
        let (name, content) = p.record[32..p.length].split_at(name_len);
        let mut r = [0u8; 100];
        r[0..4].copy_from_slice(&1u32.to_le_bytes());
        r[4..8].copy_from_slice(&outcome.to_le_bytes());
        r[8..16].copy_from_slice(&9u64.to_le_bytes());
        r[16..24].copy_from_slice(&attempt.to_le_bytes());
        r[24..32].copy_from_slice(&1u64.to_le_bytes());
        r[32..64].copy_from_slice(&crate::kernel_proposal::request_digest(core::str::from_utf8(name).unwrap(), content));
        r[64..96].copy_from_slice(&crate::sha256::sha256(&[content]));
        r
    }

    fn job(attempt: u64, number: u32, outcome: Option<u32>) -> FileJob {
        let mut p = note::prepare(attempt, number, "hello").unwrap();
        p.job.submitted().unwrap();
        if let Some(outcome) = outcome {
            let r = receipt(&p, outcome, attempt);
            note::settle(&mut p.job, &r);
        }
        p.job
    }

    #[test]
    fn each_made_file_finds_its_own_job_and_no_other() {
        let mut jobs = Jobs::new();
        assert!(jobs.last().is_none());
        // Note 1 declined, then made on a second attempt; note 2 unknown; note 3 made.
        for (attempt, number, outcome) in [(1, 1, Some(2)), (2, 1, Some(1)), (3, 2, None), (4, 3, Some(1))] {
            assert!(jobs.keep(job(attempt, number, outcome)).is_ok());
        }
        assert_eq!((jobs.len(), jobs.last().unwrap().attempt()), (4, 4));
        let first = *job(9, 1, None).destination();
        let second = *job(9, 2, None).destination();
        let third = *job(9, 3, None).destination();
        assert_eq!(jobs.made(&first).map(|j| j.attempt()), Some(2));
        assert!(jobs.made(&second).is_none());
        assert_eq!(jobs.made(&third).map(|j| j.attempt()), Some(4));
    }

    #[test]
    fn a_full_set_drops_nothing_and_takes_nothing() {
        let mut jobs = Jobs::new();
        for n in 0..KEPT as u32 {
            assert!(!jobs.is_full());
            assert!(jobs.keep(job(n as u64 + 1, n + 1, if n == 0 { None } else { Some(1) })).is_ok());
        }
        assert!(jobs.is_full());
        assert!(jobs.keep(job(99, 99, Some(1))).is_err());
        assert_eq!(jobs.len(), KEPT);
        // The first, never answered, is still there; the last kept is still the last.
        assert_eq!(jobs.kept[0].as_ref().unwrap().attempt(), 1);
        assert_eq!(jobs.last().unwrap().attempt(), KEPT as u64);
    }
}
