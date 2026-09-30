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

Tables and code blocks
----------------------

RFC, ADR, changeset, staging, design, spec, principles, glossary and
contributing documents may use two directives, in exactly these forms::

  .. list-table:: <optional title>
     :header-rows: <n>
     :widths: <w> <w> ... | auto

     * - <cell>
       - <cell>
         <continuation of the cell>
     * - <cell>
       -

  .. code-block:: <language>

     <code>

- A ``list-table`` takes only the options ``:header-rows:`` and ``:widths:``,
  each at most once, directly after the directive line; ``:widths:`` gives one
  positive integer per column, or ``auto``. A blank line separates them from
  the rows, which start at the options' column. Rows start with ``* -``,
  further cells with ``-`` two columns deeper, one space before the cell
  text, and cell text continues four columns deeper than ``*`` or more.
  Every row has as many cells as the first, and ``:header-rows:`` leaves at
  least one row below the header. Cell text follows the rest of this style.
- A ``code-block`` names one language (``text`` for plain text), takes no
  option, and is followed by a blank line and indented content.
- Both are indented with spaces, never tabs.

Backlog, phase, deliverable, state and layout documents use neither.

Not allowed
-----------

- Simple, grid and pipe tables, and every other directive (``.. note::``,
  ``.. csv-table::``, ``.. code::`` and the rest). A comment line starting
  with ``..`` is allowed.
- Substitutions, footnotes and citations.
- A link from a document under a project path into ``_palette/``.

Templates
---------

In a template, a field value or a line such as ``a | b | c`` lists the allowed
values; ``<...>`` marks text the author supplies; a value followed by
`` — <...>`` takes free text after the dash. Every field and every section a
template shows is required, in the template's order. A section with nothing to
say states ``None.``
