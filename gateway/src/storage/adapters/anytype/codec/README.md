# Page codec groundwork

This module is not connected to a writer. It renders supported flat AnyBlock
text, headings, quotes, code, checkboxes, lists, dividers and simple pipe
tables to normalized Markdown. Unknown blocks and blocks with formatting or
content that cannot be represented without loss become markers in Markdown and
JSON values in a sidecar. A missing, repeated or removed marker is refused.
The sidecar preserves the decoded JSON value, not the original JSON byte stream.

Front matter is refused. The parser is deliberately conservative and does not
cover all CommonMark or all Anytype table forms. Complex tables, styled text,
non-list indentation and unsupported AnyBlock types remain opaque. Input syntax
that cannot be represented faithfully needs an explicit rejection or an
informational conflict before enabling a writer. IDs of recognized blocks are
not retained by Markdown conversion. The patch API's `insert_blocks` shape uses
`relative_indent` and does not accept arbitrary output-only fields; sidecar
values cannot simply be sent as insert payloads. Integration must establish a
safe update strategy, validate it against the live API, and preserve existing
opaque blocks before the P5 codec can be considered complete.
