# Deploy rehearsal: mic-rtsim on a copy of OmegaSrv

Rehearsal of DECISIONS.md SRV1: copy Ben's live MIC server (OmegaSrv) read-only, prune the world to what
players built, boot it locally on the old pack, the current pack, and the current pack with mic-rtsim, and
write down what a real deploy needs. Run 2026-10-20 and 21 on the laptop against a snapshot taken
2026-10-20.

Private data stays private: the world copies, logs and player files live under `~/mic-omegasrv/` on the
laptop and are not in this repository. The tools are in `tools/deploy-rehearsal/` (stdlib Python, no
private data in them).

STATUS: DRAFT, measurements pending.
