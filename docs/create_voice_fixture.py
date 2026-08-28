import math
import wave

sample_rate = 16000
frames = bytearray()
for index in range(sample_rate):
    sample = int(1200 * math.sin(2 * math.pi * 440 * index / sample_rate))
    frames.extend(int(sample).to_bytes(2, byteorder="little", signed=True))

with wave.open("tests/fixtures_tone.wav", "wb") as audio:
    audio.setnchannels(1)
    audio.setsampwidth(2)
    audio.setframerate(sample_rate)
    audio.writeframes(frames)
