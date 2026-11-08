"""Generate synthetic packets with libopus 1.6.1; never reads audio files."""
import ctypes as c
import ctypes.util
from pathlib import Path
import sys

lib = c.CDLL(ctypes.util.find_library('opus'))
lib.opus_get_version_string.restype = c.c_char_p
assert lib.opus_get_version_string() == b'libopus 1.6.1'
for kind in ['encoder', 'decoder']:
    create = getattr(lib, f'opus_{kind}_create')
    create.restype = c.c_void_p
    create.argtypes = [c.c_int, c.c_int, c.c_int, c.POINTER(c.c_int)] if kind == 'encoder' else [c.c_int, c.c_int, c.POINTER(c.c_int)]
    getattr(lib, f'opus_{kind}_ctl').argtypes = [c.c_void_p, c.c_int]
    getattr(lib, f'opus_{kind}_destroy').argtypes = [c.c_void_p]
lib.opus_encode_float.argtypes = [c.c_void_p, c.POINTER(c.c_float), c.c_int, c.POINTER(c.c_ubyte), c.c_int]
lib.opus_decode_float.argtypes = [c.c_void_p, c.POINTER(c.c_ubyte), c.c_int, c.POINTER(c.c_float), c.c_int, c.c_int]

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
# Internal libopus test CTL: force SILK, CELT, SILK, Hybrid, CELT, Hybrid, SILK.
modes = [1000, 1002, 1000, 1001, 1002, 1001, 1000]
for channels in [1, 2]:
    err = c.c_int()
    enc = lib.opus_encoder_create(48000, channels, 2049, c.byref(err))
    assert err.value == 0
    dec = lib.opus_decoder_create(48000, channels, c.byref(err))
    assert err.value == 0
    def ctl(request, value):
        assert lib.opus_encoder_ctl(enc, request, c.c_int(value)) == 0
    ctl(4002, 48000 * channels)  # bitrate
    ctl(4006, 0)  # CBR
    ctl(4022, channels)  # forced channel count
    lines = []
    frame = 0
    for mode in modes:
        ctl(11002, mode)
        ctl(4008, 1104 if mode == 1001 else 1103)  # SWB Hybrid; WB otherwise
        for _ in range(3):
            # Integer square waves: distinct L/R signals, no libm dependency.
            pcm = (c.c_float * (960 * channels))(*[
                (0.15 if ((frame * 960 + i) // (47 + 12 * ch)) % 2 else -0.15)
                + (0.025 if ((frame * 960 + i) // (7 + ch)) % 2 else -0.025)
                for i in range(960) for ch in range(channels)
            ])
            packet = (c.c_ubyte * 1275)()
            n = lib.opus_encode_float(enc, pcm, 960, packet, 1275)
            assert n > 0
            decoded = (c.c_float * (960 * channels))()
            assert lib.opus_decode_float(dec, packet, n, decoded, 960, 0) == 960
            rng = c.c_uint32()
            assert lib.opus_decoder_ctl(dec, 4031, c.byref(rng)) == 0
            lines.append(f'{rng.value:08x} {bytes(packet[:n]).hex()}')
            frame += 1
    (out / f'transition-{channels}ch.txt').write_text('\n'.join(lines) + '\n')
    lib.opus_encoder_destroy(enc)
    lib.opus_decoder_destroy(dec)
print('generated 2 synthetic streams, 21 packets each, libopus 1.6.1')
