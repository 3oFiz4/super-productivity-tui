#!/usr/bin/env python3
"""Minimal Super Productivity Local REST API client.

Usage:
  set SP_API_TOKEN=your_token_here
  python sp_client.py health
  python sp_client.py tasks
  python sp_client.py current
  python sp_client.py add "My new task"
  python sp_client.py done <task_id>
"""

import argparse
import json
import os
import sys
import urllib.error
import urllib.request

DEFAULT_BASE_URL = "http://127.0.0.1:3876"


def request(base_url: str, method: str, path: str, token: str | None = None, body: dict | None = None) -> dict:
    url = f"{base_url.rstrip('/')}{path}"
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    data = None
    if body is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(body).encode("utf-8")

    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace")
        try:
            return json.loads(err_body)
        except json.JSONDecodeError:
            return {"ok": False, "error": {"code": f"HTTP_{e.code}", "message": err_body}}
    except urllib.error.URLError as e:
        return {"ok": False, "error": {"code": "CONNECTION_ERROR", "message": str(e.reason)}}


def main() -> int:
    parser = argparse.ArgumentParser(description="Super Productivity Local REST API client")
    parser.add_argument("--url", default=os.environ.get("SP_API_URL", DEFAULT_BASE_URL), help="Base URL (default: http://127.0.0.1:3876)")
    parser.add_argument("--token", default=os.environ.get("SP_API_TOKEN"), help="Bearer token (or set SP_API_TOKEN)")

    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("health", help="Check if Super Productivity local REST API is up")
    sub.add_parser("tasks", help="List tasks")
    sub.add_parser("current", help="Get currently active task")

    add_p = sub.add_parser("add", help="Create a new task")
    add_p.add_argument("title", help="Task title")
    add_p.add_argument("--notes", default="", help="Optional task notes")

    done_p = sub.add_parser("done", help="Mark a task as done")
    done_p.add_argument("task_id", help="Task ID")

    args = parser.parse_args()

    if args.cmd == "health":
        res = request(args.url, "GET", "/health")
    else:
        if not args.token:
            print("Error: API token required. Pass --token or set SP_API_TOKEN.", file=sys.stderr)
            return 1

        if args.cmd == "tasks":
            res = request(args.url, "GET", "/tasks", token=args.token)
        elif args.cmd == "current":
            res = request(args.url, "GET", "/task-control/current", token=args.token)
        elif args.cmd == "add":
            payload = {"title": args.title}
            if args.notes:
                payload["notes"] = args.notes
            res = request(args.url, "POST", "/tasks", token=args.token, body=payload)
        elif args.cmd == "done":
            res = request(args.url, "PATCH", f"/tasks/{args.task_id}", token=args.token, body={"isDone": True})
        else:
            return 1

    print(json.dumps(res, indent=2))
    return 0 if res.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
