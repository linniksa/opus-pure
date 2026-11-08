# Synthetic mode-transition fixtures

`transition-{1,2}ch.txt` contains 21 consecutive 20 ms packets per stream.
Each line is the hexadecimal libopus decoder final range, a space, and the
hexadecimal encoded packet. No recorded audio is used.

Generated with libopus 1.6.1 from two integer square waves, using the internal
force-mode test control to request SILK, CELT, SILK, Hybrid, CELT, Hybrid, SILK
(three frames each). The actual switch can be delayed by the encoder; inspect
the packet TOC rather than assuming that the requested mode applies immediately.

The generator and these synthetic fixtures are provided under the repository's
BSD-3-Clause license. libopus's BSD license is retained in the root `LICENSE`.

To regenerate with libopus 1.6.1 installed:

```sh
python3 reference/transitions/generate.py tests/fixtures
```

The range oracle checks entropy state at all five API rates and both output
channel counts; it is not a PCM-conformance oracle. Separate unit tests cover
the interleaved cross-fades and outgoing CELT overlap tail.
