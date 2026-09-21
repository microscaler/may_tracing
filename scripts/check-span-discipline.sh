#!/bin/sh
# check-span-discipline.sh — ADR-0001 enforcement for may code.
#
# Fails when Rust source under the given paths
#   (a) enters a span (`.enter()`, `.entered()`, `.in_scope(`) without a `// no-yield:`
#       justification on the same line or the line above, or
#   (b) with --strict: creates a span or event with tracing's macros without an explicit
#       `parent:` (a contextual parent reads the thread's span stack, which is wrong on
#       may) unless the line above carries `// root-span:` or `// root-event:`.
#       (a) is the hazard — an entered guard across a yield corrupts the thread's stack and
#       panics the process later; (b) is only mis-attribution once nothing is entered, so
#       it is opt-in until a repo has converted its events.
# Ignores tests/, examples/, benches/, target/ directories, comment lines, and any line
# mentioning `may_tracing::` (the macros there are the sanctioned form).
#
# Usage: check-span-discipline.sh [--fix-hint] [--strict] <path>...
# Exit 0 when clean, 1 on hits, 2 on usage. POSIX sh + find + awk; no ripgrep.
#
# ADR-0001: https://github.com/microscaler/may_tracing/blob/main/docs/ADR/ADR-0001-no-entered-guard-across-a-yield.md

hint=0; strict=0
while [ $# -gt 0 ]; do
  case "$1" in
    --fix-hint) hint=1; shift ;;
    --strict)   strict=1; shift ;;
    --) shift; break ;;
    -*) echo "usage: $0 [--fix-hint] [--strict] <path>..." >&2; exit 2 ;;
    *) break ;;
  esac
done
if [ $# -eq 0 ]; then echo "usage: $0 [--fix-hint] [--strict] <path>..." >&2; exit 2; fi

for root in "$@"; do
  [ -e "$root" ] || { echo "$0: no such path: $root" >&2; exit 2; }
done

# shellcheck disable=SC2086
find "$@" -type f -name '*.rs' \
    ! -path '*/tests/*' ! -path '*/examples/*' ! -path '*/benches/*' ! -path '*/target/*' \
    ! -name '.nfs*' \
| sort \
| xargs awk -v hint="$hint" -v strict="$strict" '
  function report(kind, msg) {
    printf "%s:%d: %s: %s\n", FILENAME, FNR, kind, $0
    if (hint) printf "    hint: %s\n", msg
    hits++
  }
  FNR == 1 { prev = ""; pending = "" }
  {
    line = $0
    # a span!/event! opened on the previous line: `parent:` is conventionally its first
    # argument, so one line of look-ahead decides
    if (pending != "") {
      if (line !~ /parent:/) { $0 = pline; FNR_saved = FNR; FNR = pfnr; report(pending, pmsg); FNR = FNR_saved; $0 = line }
      pending = ""
    }
    if (line ~ /^[[:space:]]*\/\//)  { prev = line; next }   # comments, doc comments
    if (line ~ /may_tracing::/)       { prev = line; next }   # the sanctioned form

    if (line ~ /\.(enter|entered)\(\)|\.in_scope\(/ && line !~ /no-yield:/ && prev !~ /no-yield:/)
      report("entered-span",
             "make the span the context instead: may_tracing::with_span(span, || ..) or let _ctx = may_tracing::set_current(span); if the section truly cannot yield, add // no-yield: <why>")

    if (strict && line ~ /(^|[^_[:alnum:]])(tracing::)?(span|trace_span|debug_span|info_span|warn_span|error_span)!\(/ \
        && line !~ /parent:/ && prev !~ /root-span:/) {
      msg = "may_tracing::child_span!(Level, \"name\", fields..) / info_span_in_current!(..), or span!(parent: None, ..) with // root-span: <why> on the line above"
      if (line ~ /!\([[:space:]]*$/) { pending = "contextual-span"; pmsg = msg; pline = line; pfnr = FNR }
      else report("contextual-span", msg)
    }

    if (strict && line ~ /(^|[^_[:alnum:]])(tracing::)?(event|trace|debug|info|warn|error)!\(/ \
        && line !~ /parent:/ && prev !~ /root-event:/) {
      msg = "may_tracing::info_in_current!(..) (or the level you need), or event!(parent: None, ..) with // root-event: <why> on the line above"
      if (line ~ /!\([[:space:]]*$/) { pending = "contextual-event"; pmsg = msg; pline = line; pfnr = FNR }
      else report("contextual-event", msg)
    }

    prev = line
  }
  END {
    if (hits > 0) {
      printf "check-span-discipline: %d hit(s). ADR-0001: no entered guard across a yield; explicit parents.\n", hits > "/dev/stderr"
      exit 1
    }
  }'
rc=$?
# xargs reports a non-zero awk exit as 123
[ "$rc" -eq 0 ] && exit 0
exit 1
