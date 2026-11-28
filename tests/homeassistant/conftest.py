"""Home Assistant tests for custom_components/rustdo, against a running RustDo Hub.

    cargo run --release -p rustdo-hub -- --port 7979 --data /tmp/hub   (in hub/)
    RUSTDO_HUB_URL=http://127.0.0.1:7979 pytest tests/homeassistant
"""

import os
import sys
from pathlib import Path

import pytest

# The repository root, so `custom_components.rustdo` imports.
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

HUB = os.environ.get("RUSTDO_HUB_URL", "http://127.0.0.1:7979")


@pytest.fixture(autouse=True)
def auto_enable_custom_integrations(enable_custom_integrations, socket_enabled):
    # These tests talk to a real hub, so network sockets stay allowed.
    yield


@pytest.fixture
def hub_url():
    return HUB
