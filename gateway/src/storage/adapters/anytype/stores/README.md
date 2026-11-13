# Typed cache groundwork

This cache is not wired to the engine, Settings, Bot stores or Anytype writes.
It does not enable P4 or change schema readiness. It loads a type-scoped poll
and hydrates each returned object using the shared client. Stream additions,
updates and removals must all call `invalidate`; stream rows are only hints.
Failed or superseded polls do not replace the cache. A row missing from search
is retained as uncertain rather than silently deleted. An unknown invalidated
id stays unresolved until observed. Freshness is bounded by the caller's TTL.
Confirmed write-through accepts only a complete GET after a successful write,
with a token and start time captured before that GET; a create/patch acknowledgement alone is
not a complete object. Callers must preserve and retry outbox operations on
failure or supersession.

Search is paginated and not atomic. The strict search path rejects ambiguous
pagination and results exceeding 10,000 rows. A completed poll is not proof of a
transactionally complete remote snapshot; callers must not turn cache absence
into a remote/local delete. Archive, deletion verification, durable outbox,
echo guards, singleton/natural-key collision policy, stream wiring, polling
scheduler, offline persistence and Settings integration remain to be built.
The live shape and type-key mapping of hydrated objects must be verified against
a scratch space before enabling a typed writer.
