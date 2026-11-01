//! Native adjustments preserve the source baseline and append immutable receipts.
use super::*;
const NATIVE_MAX: usize = 4 * 1024 * 1024;
fn command(id: &str, expected: &str, operator: &str) -> Result<()> {
    if !valid_id(id) || !digest(expected) || operator.trim().is_empty() || !text(operator, 200) {
        return Err(bad("Request id, expected head and operator label required"));
    }
    Ok(())
}
fn journal_check(j: &NativeJournal, accounts: &[Account], locked: Option<&str>) -> Result<()> {
    if j.format != "nxt-native-journal/v1"
        || !valid_id(&j.id)
        || !valid_id(&j.request_id)
        || !date(&j.date)
        || !text(&j.memo, 2000)
        || j.lines.len() < 2
        || j.lines.len() > 500
        || encoded(j)?.len() > 256 * 1024
    {
        return Err(bad("Invalid or oversized native journal"));
    }
    if locked.is_some_and(|d| j.date.as_str() <= d) {
        return Err(err(
            StatusCode::CONFLICT,
            "Journal date falls in a locked period",
        ));
    }
    let (mut debit, mut credit) = (0i128, 0i128);
    for l in &j.lines {
        if !accounts.iter().any(|a| a.id == l.account_id) || !text(&l.memo, 2000) {
            return Err(bad(
                "Native line references undefined company account or invalid memo",
            ));
        }
        let (d, c) = (money(&l.debit_cents)?, money(&l.credit_cents)?);
        if (d == 0 && c == 0) || (d > 0 && c > 0) {
            return Err(bad(
                "Native line must have exactly one positive debit or credit",
            ));
        }
        debit = debit.checked_add(d).ok_or_else(|| bad("Amount overflow"))?;
        credit = credit
            .checked_add(c)
            .ok_or_else(|| bad("Amount overflow"))?;
    }
    if debit != credit || debit == 0 {
        return Err(bad("Native journal must balance with positive amounts"));
    }
    Ok(())
}
fn entry(j: &NativeJournal) -> Entry {
    Entry {
        id: j.id.clone(),
        date: j.date.clone(),
        source_id: format!("nxt-native:{}", j.id),
        memo: j.memo.clone(),
        lines: j
            .lines
            .iter()
            .map(|l| Line {
                account_id: l.account_id.clone(),
                debit_cents: l.debit_cents.clone(),
                credit_cents: l.credit_cents.clone(),
                source_row: 0,
                memo: l.memo.clone(),
                source_metadata: Some(
                    json!({"origin":"native_adjustment","request_id":j.request_id}),
                ),
            })
            .collect(),
        source_metadata: Some(
            json!({"origin":"native_adjustment","request_id":j.request_id,"reversal_of":j.reversal_of}),
        ),
    }
}
fn lines(e: &Entry) -> Vec<NativeLine> {
    e.lines
        .iter()
        .filter(|l| l.debit_cents != "0" || l.credit_cents != "0")
        .map(|l| NativeLine {
            account_id: l.account_id.clone(),
            debit_cents: l.credit_cents.clone(),
            credit_cents: l.debit_cents.clone(),
            memo: l.memo.clone(),
        })
        .collect()
}
impl CompanyBooks {
    pub(super) async fn head_chain(
        &self,
        company: &str,
        id: &str,
        h: &Head,
    ) -> Result<Vec<(String, Head)>> {
        let mut result = vec![(id.to_string(), h.clone())];
        let mut next = h.previous_head.clone();
        while let Some(id) = next {
            if result.len() >= 129 {
                return Err(corrupt());
            }
            let ancestor: Head = self
                .object(&Self::key(company, "receipt", &id)?, &id)
                .await
                .map_err(|e| {
                    if e.0 == StatusCode::SERVICE_UNAVAILABLE {
                        e
                    } else {
                        corrupt()
                    }
                })?;
            next = ancestor.previous_head.clone();
            result.push((id, ancestor));
        }
        result.reverse();
        Ok(result)
    }
    pub(super) async fn validate_chain(
        &self,
        company: &str,
        id: &str,
        h: &Head,
        p: &Preview,
    ) -> Result<Vec<Entry>> {
        let (accounts, entries) = self.data(company, &p.input.chunks).await?;
        self.validate_chain_data(company, id, h, p, &accounts, &entries)
            .await
    }
    pub(super) async fn validate_chain_data(
        &self,
        company: &str,
        id: &str,
        h: &Head,
        p: &Preview,
        accounts: &[Account],
        source_entries: &[Entry],
    ) -> Result<Vec<Entry>> {
        let chain = self.head_chain(company, id, h).await?;
        let base = &chain[0].1;
        if base.format != "nxt-company-book-head/v1"
            || base.revision != 1
            || base.previous_head.is_some()
            || base.operation.is_some()
            || base.locked_through
                != p.input
                    .source
                    .period_lock
                    .as_ref()
                    .and_then(|p| p.closed_through.clone())
            || !base.journal_chunks.is_empty()
            || p.input.expected_head.is_some()
        {
            return Err(corrupt());
        }
        let mut all: BTreeMap<String, Entry> = source_entries
            .iter()
            .cloned()
            .map(|e| (e.id.clone(), e))
            .collect();
        let mut journals = vec![];
        let mut ids = HashSet::new();
        let mut reversed = HashSet::new();
        let mut size = 0usize;
        let mut prior: Option<&Head> = None;
        let mut prior_id: Option<&str> = None;
        for (receipt_id, head) in &chain {
            if head.company_id != company
                || head.preview_id != p_hash(p)?
                || !valid_id(&head.commit_request_id)
                || !ids.insert(&head.commit_request_id)
                || head.reviewer_label.trim().is_empty()
                || !text(&head.reviewer_label, 200)
                || chrono::DateTime::parse_from_rfc3339(&head.committed_at).is_err()
            {
                return Err(corrupt());
            }
            if let Some(prev) = prior {
                if head.format != "nxt-company-book-head/v2"
                    || head.revision != prev.revision + 1
                    || head.previous_head.as_deref() != prior_id
                {
                    return Err(corrupt());
                }
                let mut chunks = prev.journal_chunks.clone();
                let mut locked = prev.locked_through.clone();
                match head.operation.as_ref().ok_or_else(corrupt)? {
                    Operation::Journal {
                        input,
                        journal_sha256,
                    } => {
                        command(
                            &input.request_id,
                            &input.expected_head,
                            &input.operator_label,
                        )
                        .map_err(|_| corrupt())?;
                        if input.expected_head != prior_id.unwrap()
                            || input.request_id != head.commit_request_id
                            || input.operator_label != head.reviewer_label
                        {
                            return Err(corrupt());
                        }
                        let j: NativeJournal = self
                            .object(
                                &Self::key(company, "native", journal_sha256)?,
                                journal_sha256,
                            )
                            .await
                            .map_err(|_| corrupt())?;
                        let expected = native(company, input, None)?;
                        if j != expected {
                            return Err(corrupt());
                        }
                        journal_check(&j, &accounts, locked.as_deref()).map_err(|_| corrupt())?;
                        if all.insert(j.id.clone(), entry(&j)).is_some() {
                            return Err(corrupt());
                        }
                        size += encoded(&j)?.len();
                        chunks.push(journal_sha256.clone());
                        journals.push(j);
                    }
                    Operation::Reversal {
                        input,
                        entry_id,
                        journal_sha256,
                    } => {
                        command(
                            &input.request_id,
                            &input.expected_head,
                            &input.operator_label,
                        )
                        .map_err(|_| corrupt())?;
                        if input.expected_head != prior_id.unwrap()
                            || input.request_id != head.commit_request_id
                            || input.operator_label != head.reviewer_label
                            || !reversed.insert(entry_id.clone())
                        {
                            return Err(corrupt());
                        }
                        let target = all.get(entry_id).ok_or_else(corrupt)?;
                        let j: NativeJournal = self
                            .object(
                                &Self::key(company, "native", journal_sha256)?,
                                journal_sha256,
                            )
                            .await
                            .map_err(|_| corrupt())?;
                        let ji = JournalInput {
                            request_id: input.request_id.clone(),
                            expected_head: input.expected_head.clone(),
                            operator_label: input.operator_label.clone(),
                            date: input.date.clone(),
                            memo: input.memo.clone(),
                            lines: lines(target),
                        };
                        if j != native(company, &ji, Some(entry_id.clone()))? {
                            return Err(corrupt());
                        }
                        journal_check(&j, &accounts, locked.as_deref()).map_err(|_| corrupt())?;
                        if all.insert(j.id.clone(), entry(&j)).is_some() {
                            return Err(corrupt());
                        }
                        size += encoded(&j)?.len();
                        chunks.push(journal_sha256.clone());
                        journals.push(j);
                    }
                    Operation::PeriodLock { input } => {
                        command(
                            &input.request_id,
                            &input.expected_head,
                            &input.operator_label,
                        )
                        .map_err(|_| corrupt())?;
                        if input.expected_head != prior_id.unwrap()
                            || input.request_id != head.commit_request_id
                            || input.operator_label != head.reviewer_label
                            || !date(&input.locked_through)
                            || input.note.trim().is_empty()
                            || !text(&input.note, 2000)
                            || locked
                                .as_deref()
                                .is_some_and(|d| input.locked_through.as_str() <= d)
                        {
                            return Err(corrupt());
                        }
                        locked = Some(input.locked_through.clone());
                    }
                }
                if head.journal_chunks != chunks
                    || head.locked_through != locked
                    || size > NATIVE_MAX
                {
                    return Err(corrupt());
                }
            }
            prior = Some(head);
            prior_id = Some(receipt_id.as_str());
        }
        Ok(journals.iter().map(entry).collect())
    }
    async fn replay_operation(
        &self,
        company: &str,
        id: &str,
        h: &Head,
        request: &str,
        op: &Operation,
    ) -> Result<Option<Value>> {
        for (rid, r) in self.head_chain(company, id, h).await? {
            if r.commit_request_id == request {
                if r.operation.as_ref() == Some(op) {
                    return Ok(Some(
                        json!({"head_sha256":rid,"head":r,"current_head_sha256":id,"idempotent_replay":true,"controls":controls()}),
                    ));
                }
                return Err(err(
                    StatusCode::CONFLICT,
                    "Request id already used with different command content",
                ));
            }
        }
        Ok(None)
    }
    async fn append_operation(
        &self,
        company: &str,
        expected: &str,
        request: &str,
        operator: &str,
        op: Operation,
    ) -> Result<Value> {
        let (id, h) = self
            .current(company)
            .await?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Company has no committed book"))?;
        if let Some(result) = self
            .replay_operation(company, &id, &h, request, &op)
            .await?
        {
            return Ok(result);
        }
        if id != expected {
            return Err(err(
                StatusCode::CONFLICT,
                "Company head changed; review current book before posting",
            ));
        }
        if h.revision >= 129 {
            return Err(err(
                StatusCode::CONFLICT,
                "Bounded native receipt limit reached",
            ));
        }
        let mut chunks = h.journal_chunks.clone();
        let mut locked = h.locked_through.clone();
        match &op {
            Operation::Journal { journal_sha256, .. }
            | Operation::Reversal { journal_sha256, .. } => chunks.push(journal_sha256.clone()),
            Operation::PeriodLock { input } => locked = Some(input.locked_through.clone()),
        }
        let next = Head {
            format: "nxt-company-book-head/v2".into(),
            company_id: company.into(),
            preview_id: h.preview_id.clone(),
            revision: h.revision + 1,
            committed_at: Utc::now().to_rfc3339(),
            commit_request_id: request.into(),
            reviewer_label: operator.into(),
            previous_head: Some(id.clone()),
            journal_chunks: chunks,
            locked_through: locked,
            operation: Some(op.clone()),
            review: h.review.clone(),
        };
        let p = self.load_preview(company, &h.preview_id).await?;
        self.validate_chain(company, "prospective", &next, &p)
            .await?;
        let raw = encoded(&next)?;
        let next_id = hash(raw.as_bytes());
        self.immutable(&Self::key(company, "receipt", &next_id)?, &next)
            .await?;
        if !self
            .store
            .compare_exchange(&Self::key(company, "head", "v1")?, Some(&id), &next_id)
            .await
            .map_err(storage)?
        {
            let (latest_id, latest) = self.current(company).await?.ok_or_else(corrupt)?;
            if let Some(result) = self
                .replay_operation(company, &latest_id, &latest, request, &op)
                .await?
            {
                return Ok(result);
            }
            return Err(err(
                StatusCode::CONFLICT,
                "Concurrent company journal change; no second posting occurred",
            ));
        }
        Ok(
            json!({"head_sha256":next_id,"head":next,"idempotent_replay":false,"controls":controls()}),
        )
    }
    pub async fn journal(&self, company: &str, input: JournalInput) -> Result<Value> {
        command(
            &input.request_id,
            &input.expected_head,
            &input.operator_label,
        )?;
        let j = native(company, &input, None)?;
        let raw = encoded(&j)?;
        let jid = hash(raw.as_bytes());
        let op = Operation::Journal {
            input: input.clone(),
            journal_sha256: jid.clone(),
        };
        let (id, h) = self
            .current(company)
            .await?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Company has no committed book"))?;
        if let Some(v) = self
            .replay_operation(company, &id, &h, &input.request_id, &op)
            .await?
        {
            return Ok(v);
        }
        if id != input.expected_head {
            return Err(err(StatusCode::CONFLICT, "Stale expected company head"));
        }
        let p = self.load_preview(company, &h.preview_id).await?;
        let (a, _) = self.data(company, &p.input.chunks).await?;
        journal_check(&j, &a, h.locked_through.as_deref())?;
        self.immutable(&Self::key(company, "native", &jid)?, &j)
            .await?;
        self.append_operation(
            company,
            &input.expected_head,
            &input.request_id,
            &input.operator_label,
            op,
        )
        .await
    }
    pub async fn reverse(
        &self,
        company: &str,
        target_id: &str,
        input: ReverseInput,
    ) -> Result<Value> {
        command(
            &input.request_id,
            &input.expected_head,
            &input.operator_label,
        )?;
        let (id, h) = self
            .current(company)
            .await?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Company has no committed book"))?;
        // Reconstruct target against immutable history even for retries after later operations.
        let (_, _, a, e) = self.book(company).await?;
        let target = e.iter().find(|e| e.id == target_id).ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "Reversal target not found in this company",
            )
        })?;
        let ji = JournalInput {
            request_id: input.request_id.clone(),
            expected_head: input.expected_head.clone(),
            operator_label: input.operator_label.clone(),
            date: input.date.clone(),
            memo: input.memo.clone(),
            lines: lines(target),
        };
        let j = native(company, &ji, Some(target_id.into()))?;
        let jid = hash(encoded(&j)?.as_bytes());
        let op = Operation::Reversal {
            input: input.clone(),
            entry_id: target_id.into(),
            journal_sha256: jid.clone(),
        };
        if let Some(v) = self
            .replay_operation(company, &id, &h, &input.request_id, &op)
            .await?
        {
            return Ok(v);
        }
        if id != input.expected_head {
            return Err(err(StatusCode::CONFLICT, "Stale expected company head"));
        }
        for (_, r) in self.head_chain(company, &id, &h).await? {
            if matches!(r.operation,Some(Operation::Reversal{entry_id,..})if entry_id==target_id) {
                return Err(err(
                    StatusCode::CONFLICT,
                    "Entry already has a recorded reversal",
                ));
            }
        }
        journal_check(&j, &a, h.locked_through.as_deref())?;
        self.immutable(&Self::key(company, "native", &jid)?, &j)
            .await?;
        self.append_operation(
            company,
            &input.expected_head,
            &input.request_id,
            &input.operator_label,
            op,
        )
        .await
    }
    pub async fn lock_period(&self, company: &str, input: LockInput) -> Result<Value> {
        command(
            &input.request_id,
            &input.expected_head,
            &input.operator_label,
        )?;
        if !date(&input.locked_through) || input.note.trim().is_empty() || !text(&input.note, 2000)
        {
            return Err(bad("Valid lock date and note required"));
        }
        let (id, h) = self
            .current(company)
            .await?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Company has no committed book"))?;
        let op = Operation::PeriodLock {
            input: input.clone(),
        };
        if let Some(v) = self
            .replay_operation(company, &id, &h, &input.request_id, &op)
            .await?
        {
            return Ok(v);
        }
        if h.locked_through
            .as_deref()
            .is_some_and(|d| input.locked_through.as_str() <= d)
        {
            return Err(err(
                StatusCode::CONFLICT,
                "Period lock can only advance; unlocking requires a separate policy",
            ));
        }
        self.append_operation(
            company,
            &input.expected_head,
            &input.request_id,
            &input.operator_label,
            op,
        )
        .await
    }
}
fn p_hash(p: &Preview) -> Result<String> {
    Ok(hash(encoded(p)?.as_bytes()))
}
fn native(
    company: &str,
    input: &JournalInput,
    reversal_of: Option<String>,
) -> Result<NativeJournal> {
    let id = hash(encoded(&(company, input, &reversal_of))?.as_bytes());
    Ok(NativeJournal {
        format: "nxt-native-journal/v1".into(),
        company_id: company.into(),
        id: format!("nxtj_{id}"),
        date: input.date.clone(),
        memo: input.memo.clone(),
        lines: input.lines.clone(),
        reversal_of,
        request_id: input.request_id.clone(),
    })
}
