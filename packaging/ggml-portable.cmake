# Release builds run on other people's machines: build whisper.cpp's CPU code for
# a common x86-64 baseline (AVX2, FMA, F16C, BMI2: CPUs from 2013 on) instead of
# the CI runner's own CPU, which may have AVX-512. Passed to the cmake crate via
# CMAKE_TOOLCHAIN_FILE in .github/workflows/release.yml.
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
