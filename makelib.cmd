@echo off
cl /c /MD /GA /GF /Gy /O2 /arch:SSE optimize/memcpy__amd.cpp optimize/memcpy_amd.c
lib /machine:ix86 /out:memcpy_amd.lib memcpy__amd.obj memcpy_amd.obj
