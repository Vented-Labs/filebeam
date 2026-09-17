#!/usr/bin/env python3
"""Require the actual peer receive case, rather than accepting a skipped suite."""

import json
import sys
from pathlib import Path

METHOD = "testHTTPSFixtureDownloadsThroughExplicitOriginConfirmation"


def peer_passed(node):
    if isinstance(node, list):
        return any(peer_passed(child) for child in node)
    if not isinstance(node, dict):
        return False
    identities = (node.get(key) for key in ("name", "nodeIdentifier", "testIdentifier", "identifier"))
    if any(isinstance(value, str) and METHOD in value for value in identities):
        if node.get("result") == "Passed" or node.get("testStatus") == "Success":
            return True
    return any(peer_passed(child) for child in node.values() if isinstance(child, (dict, list)))


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        passed = {"name": METHOD, "result": "Passed"}
        skipped = {"name": METHOD, "result": "Skipped"}
        assert peer_passed({"testNodes": [passed]})
        assert not peer_passed({"testNodes": [skipped, {"name": "testDraft", "result": "Passed"}]})
        assert not peer_passed({"name": METHOD, "testNodes": [{"name": "other", "result": "Passed"}]})
        assert not peer_passed({"testNodes": [{"name": METHOD, "result": "Failed"}]})
        return
    if len(sys.argv) != 2:
        raise SystemExit("usage: require-peer-test.py <xcresult-tests.json> | --self-test")
    if not peer_passed(json.loads(Path(sys.argv[1]).read_text())):
        raise SystemExit("HTTPS peer receive was not reported PASS; UI acceptance is incomplete")


if __name__ == "__main__":
    main()
