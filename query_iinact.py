#!/usr/bin/env python3
import asyncio
import json
import re
import argparse
import signal

import websockets

NUM_RE = re.compile(r"[^0-9.+-]")

def to_float(s):
    if s is None:
        return 0.0
    if isinstance(s, (int, float)):
        return float(s)
    s = NUM_RE.sub('', str(s))
    try:
        return float(s) if s else 0.0
    except ValueError:
        return 0.0

def safe_get(d, *keys, default=None):
    for k in keys:
        if k in d:
            return d[k]
        for kk in d.keys():
            if kk.lower() == str(k).lower():
                return d[kk]
    return default

def format_table(rows, headers):
    colw = [len(h) for h in headers]
    for r in rows:
        for i, cell in enumerate(r):
            colw[i] = max(colw[i], len(str(cell)))
    out = []
    out.append(" | ".join(h.ljust(colw[i]) for i,h in enumerate(headers)))
    out.append("-+-".join("-"*colw[i] for i in range(len(headers))))
    for r in rows:
        out.append(" | ".join(str(r[i]).ljust(colw[i]) for i in range(len(headers))))
    return "\n".join(out)

async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ws", default="ws://127.0.0.1:10501/ws", help="IINACT WebSocket URL")
    ap.add_argument("--once", action="store_true", help="Exit after first CombatData event")
    ap.add_argument("--top", type=int, default=8, help="How many combatants to show")
    ap.add_argument("--show-logline", action="store_true", help="Print LogLine summaries too")
    ap.add_argument("--sniff", action="store_true", help="Print raw keys for Encounter and one Combatant, then exit")
    ap.add_argument("--raw-once", action="store_true", help="Print first CombatData event as raw JSON and exit")
    args = ap.parse_args()

    stop = asyncio.Event()
    def _sig(*_):
        stop.set()
    signal.signal(signal.SIGINT, _sig)
    signal.signal(signal.SIGTERM, _sig)

    async with websockets.connect(args.ws) as ws:
        print(f"Connected to {args.ws}")
        await ws.send(json.dumps({"call":"getLanguage"}))
        lang = await ws.recv()
        print("getLanguage reply:", lang)

        await ws.send(json.dumps({"call":"subscribe", "events":["CombatData","LogLine"]}))
        print("Subscribed, waiting for events... (Ctrl-C to quit)")

        while not stop.is_set():
            try:
                raw = await asyncio.wait_for(ws.recv(), timeout=60.0)
            except asyncio.TimeoutError:
                print("No data for 60s... still connected.")
                continue
            try:
                msg = json.loads(raw)
            except json.JSONDecodeError:
                print("[Non-JSON]", raw[:200])
                continue

            t = msg.get("type")
            if t == "CombatData":
                if args.raw_once:
                    print(json.dumps(msg, indent=2)[:20000])
                    break
                enc = msg.get("Encounter", {}) or {}
                zone = safe_get(enc, "CurrentZoneName", "zone", default="")
                name = safe_get(enc, "title", "Encounter", default="")
                dur  = safe_get(enc, "duration", default="?")
                dps  = safe_get(enc, "encdps", "ENCDPS", "DPS", default="?")
                dmg  = safe_get(enc, "damage", "damageTotal", default="?")
                print(f"\n=== Encounter: {name}  Zone: {zone}  Duration: {dur}  ENCDPS: {dps}  Damage: {dmg} ===")

                if args.sniff:
                    print("Encounter keys:", sorted(list(enc.keys())))

                comb = msg.get("Combatant", {}) or {}
                rows = []
                first_stats_keys = None
                for pname, stats in comb.items():
                    if first_stats_keys is None:
                        first_stats_keys = sorted(list(stats.keys()))
                    pdps = safe_get(stats, "encdps", "ENCDPS", "dps", default="0")
                    crit = safe_get(stats, "crithit%", "Crit%", "crithit", default="")
                    dh   = safe_get(stats, "DirectHitPct", "DirectHit%", "DirectHit", "Direct%","DH%", default="")
                    deaths = safe_get(stats, "deaths", "Deaths", default="0")
                    job  = safe_get(stats, "Job", "job", default="")
                    dmg_p = safe_get(stats, "damage", "Damage", default="0")
                    rows.append([pname, job, pdps, dmg_p, crit, dh, deaths])
                rows.sort(key=lambda r: to_float(r[2]), reverse=True)
                if args.top:
                    rows = rows[:args.top]
                if args.sniff and first_stats_keys is not None:
                    print("Combatant keys (example):", first_stats_keys)
                    break
                headers = ["Name", "Job", "ENCDPS", "Damage", "Crit%", "DH%", "Deaths"]
                print(format_table(rows, headers))

                if args.once:
                    break

            elif t == "LogLine" and args.show_logline:
                line = msg.get("line") or msg.get("rawLine") or ""
                print("[LogLine]", (line[:160] + ("..." if len(line)>160 else "")))
            else:
                pass

if __name__ == "__main__":
    asyncio.run(main())

