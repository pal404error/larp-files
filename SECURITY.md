# Security policy

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting on this repository
(Security tab → Report a vulnerability) rather than opening a public
issue, so a fix can ship before details are public.

## Scope notes

- `.larp` files are data, not code: the reader never executes file
  contents. Still, treat files from untrusted sources as untrusted input —
  the parser is fuzz-lite tested against corrupt and hostile inputs.
- A single record decompressing beyond 16 MiB is rejected (zip-bomb
  guard); a torn or corrupt tail is ignored, never trusted.
- Dependencies are audited in CI (`cargo audit`). Known advisories in a
  dependency block releases until they are fixed or explicitly assessed.
