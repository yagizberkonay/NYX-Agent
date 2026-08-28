#!/usr/bin/env python3
"""NYX local browser sidecar. Commands are JSON objects, one per stdin line."""
import asyncio
import base64
import json
import os
import sys
from pathlib import Path

from playwright.async_api import async_playwright


async def main() -> None:
    headless = os.getenv("NYX_BROWSER_HEADLESS", "1") != "0"
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(headless=headless)
        context = await browser.new_context(accept_downloads=False)
        page = await context.new_page()
        for line in sys.stdin:
            if not line.strip():
                continue
            try:
                request = json.loads(line)
                action = request.get("action")
                if action == "goto":
                    url = request["url"]
                    if not (url.startswith("https://") or url.startswith("http://")):
                        raise ValueError("only HTTP(S) URLs are allowed")
                    await page.goto(url, wait_until="domcontentloaded", timeout=30_000)
                    result = {"url": page.url, "title": await page.title()}
                elif action == "read":
                    result = {"url": page.url, "title": await page.title(), "text": (await page.locator("body").inner_text())[:100_000]}
                elif action == "click":
                    await page.get_by_text(request["text"], exact=request.get("exact", False)).first.click(timeout=15_000)
                    result = {"url": page.url}
                elif action == "fill":
                    await page.locator(request["selector"]).fill(request["value"], timeout=15_000)
                    result = {"selector": request["selector"], "filled": True}
                elif action == "screenshot":
                    path = Path(request.get("path", "/tmp/nyx-browser.png")).resolve()
                    path.parent.mkdir(parents=True, exist_ok=True)
                    await page.screenshot(path=str(path), full_page=request.get("full_page", False))
                    result = {"path": str(path), "base64": base64.b64encode(path.read_bytes()).decode("ascii") if request.get("include_base64") else None}
                elif action == "back":
                    await page.go_back(wait_until="domcontentloaded", timeout=30_000)
                    result = {"url": page.url, "title": await page.title()}
                elif action == "close":
                    result = {"closed": True}
                    print(json.dumps({"ok": True, "result": result}, ensure_ascii=False), flush=True)
                    break
                else:
                    raise ValueError(f"unsupported browser action: {action}")
                print(json.dumps({"ok": True, "result": result}, ensure_ascii=False), flush=True)
            except Exception as error:
                print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=False), flush=True)
        await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
