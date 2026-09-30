RST house style
===============

Every palette document, internal or under a project path, is
reStructuredText in the subset below. The palette server checks it.

Allowed
-------

- Section titles underlined, never overlined, in this order of depth:
  ``=``, ``-``, ``~``, ``^``, ``"``. An underline is at least as long as its
  title.
- Field lists for metadata (``:Status: Accepted``); a long value continues on
  indented lines.
- Bullet lists (``-``), numbered lists (``1.``), definition lists.
- Literal blocks introduced by ``::`` and inline literals in double backquotes.
- Links written as ```text <relative/path.rst>`_``, relative to the linking
  file.

Not allowed
-----------

- Tables of any kind, and directives (``.. list-table::``, ``.. note::`` and
  the rest). A comment line starting with ``..`` is allowed.
- Substitutions, footnotes and citations.
- A link from a document under a project path into ``_palette/``.

Templates
---------

In a template, a field value or a line such as ``a | b | c`` lists the allowed
values; ``<...>`` marks text the author supplies; a value followed by
`` — <...>`` takes free text after the dash. Every field and every section a
template shows is required, in the template's order. A section with nothing to
say states ``None.``
