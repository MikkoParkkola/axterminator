#!/bin/bash
# MCP Server wrapper for axterminator
cd "$(dirname "$0")"
exec uv run python server.py
