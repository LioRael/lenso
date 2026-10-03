# Simulator/CI consumer candidate delivery

This branch transports evidence only. Do not integrate this handoff branch into main.

Source candidate: `candidate/simulator-ci-consumer/20261003-1` at
`531d5dd1edd8abedc552bf43247fb0615a26a400`.
It includes frozen Store candidate `225c0d27a0c3ae0181109041409a6f1149359702`
plus the third slice, retaining `quality needs: store-contract`.

Exact candidate CI: https://github.com/LioRael/lenso/actions/runs/37106147968
Qualification was in progress at package creation; a push is not a passing gate.
Only Core owner integrates main after exact-source required gates pass.

The ZIP includes an incremental third-slice patch, full patch against main c28,
Git bundle including the actual measured pre-integration source commit,
original measurement logs, independent exact-candidate PASS and SHA-256 index.
The repo source also includes normalized log copies and the repeatable harness.

Measured retained real consumer: baseline median 13.312937s; warm candidate
0.642470s. Initially empty target 13.067747s has no meaningful cold gain.
This proves only the selected serial component reduction; Actions transfer,
main seed/restore and whole CI critical-path benefit remain unmeasured.

ZIP SHA-256: `3b71008fe0c53c5a8928c6a8df1f8b17c47d45da335af33b6cc7f82501bfeeb3`.
