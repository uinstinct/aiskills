"""Unit tests for src/internal/lib.py helpers.

Run from the repo root with:

    uv run --project src/internal python -m unittest discover -s src/internal -p "test_*.py"
"""

from __future__ import annotations

import sys
import unittest
from unittest.mock import patch

import lib


class PromptMultiChoiceTests(unittest.TestCase):
    options = ["claude-code", "codex", "opencode", "codebuff"]

    def _run_with_inputs(self, inputs: list[str]) -> list[str]:
        """Invoke prompt_multi_choice with ``input()`` returning each value in order."""
        iterator = iter(inputs)
        with patch.object(sys.stdin, "isatty", return_value=True), patch(
            "builtins.input", side_effect=lambda *_: next(iterator)
        ):
            return lib.prompt_multi_choice("pick:", self.options)

    def test_empty_input_returns_empty_list(self):
        self.assertEqual(self._run_with_inputs([""]), [])

    def test_whitespace_only_returns_empty_list(self):
        self.assertEqual(self._run_with_inputs(["   "]), [])

    def test_single_index_selection(self):
        self.assertEqual(self._run_with_inputs(["2"]), ["codex"])

    def test_single_label_selection(self):
        self.assertEqual(self._run_with_inputs(["codebuff"]), ["codebuff"])

    def test_multiple_indices(self):
        self.assertEqual(
            self._run_with_inputs(["1,3"]), ["claude-code", "opencode"]
        )

    def test_multiple_labels_whitespace_tolerant(self):
        self.assertEqual(
            self._run_with_inputs(["claude-code, codex"]),
            ["claude-code", "codex"],
        )

    def test_mixed_indices_and_labels(self):
        self.assertEqual(
            self._run_with_inputs(["1, codebuff"]),
            ["claude-code", "codebuff"],
        )

    def test_duplicate_tokens_dedupe_preserve_order(self):
        self.assertEqual(
            self._run_with_inputs(["2, codex, 2"]),
            ["codex"],
        )

    def test_invalid_label_then_valid_reprompts(self):
        self.assertEqual(self._run_with_inputs(["cursor", "2"]), ["codex"])

    def test_index_out_of_range_then_valid_reprompts(self):
        self.assertEqual(self._run_with_inputs(["99", "1"]), ["claude-code"])

    def test_non_tty_raises_ingest_error(self):
        with patch.object(sys.stdin, "isatty", return_value=False):
            with self.assertRaises(lib.IngestError):
                lib.prompt_multi_choice("pick:", self.options)


if __name__ == "__main__":
    unittest.main()
