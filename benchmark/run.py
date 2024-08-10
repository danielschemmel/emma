#!/usr/bin/env python3

import json
import multiprocessing
import numpy
import os
import re
import scipy
import shutil
import subprocess
from matplotlib import pyplot as plt
from pathlib import Path
from subprocess import run

BENCHMARKS = [
	("chaos", []),
	("hoard/cache-scratch", [str(multiprocessing.cpu_count()), "50", "30000", "32", "1"]),
	("hoard/cache-thrash", [str(multiprocessing.cpu_count()), "50", "30000", "32", "1"]),
	("hoard/threadtest", [str(multiprocessing.cpu_count())]),
]

ALLOCATORS = [
	"emma-tls",
	"emma-clean-tls",
#	"std",
#	"libc",
#	"jemalloc",
#	"mimalloc",
]

WARMUP = 2
RUNS = 50

DIR = Path(__file__).absolute().parent
BIN_DIR = DIR / "bin"
PLOT_DIR = DIR / "plots"

STDOUT_FORMAT = re.compile(r"\D*(?P<secs>\d+(?:\.\d*)?)\D*")
TIME_FORMAT = re.compile(r"(?P<kbs>\d+)\n")
def measure(target, args):
	time_file = str(target) + ".time"
	stdout_str = run(["time", "--format=%M", f"--output={time_file}", target] + args, check=True, stdout=subprocess.PIPE).stdout.decode("utf-8")

	stdout_match = STDOUT_FORMAT.fullmatch(stdout_str)
	assert(stdout_match)

	with open(time_file, "r") as f:
		time_str = f.read()
	time_match = TIME_FORMAT.fullmatch(time_str)
	assert time_match

	return (float(stdout_match.group("secs")) * 1000, float(time_match.group("kbs")) / 1024)

def stats(sample_data, confidence_level=0.99):
	sample_mean = numpy.mean(sample_data)

	if len(sample_data) < 30:
		confidence_interval = scipy.stats.t.interval(confidence_level, len(sample_data) - 1, scale=scipy.stats.sem(sample_data))
	else:
		confidence_interval = scipy.stats.norm.interval(confidence_level, scale=scipy.stats.sem(sample_data))
	assert len(confidence_interval) == 2

	return (sample_mean, confidence_interval)

def compile_bench(benchmark, allocator):
	benchmark = Path(benchmark)

	allocator_feature = allocator
	git_stash_pop = False
	if allocator.startswith("emma-clean-"):
		allocator_feature = "emma-" + allocator[len("emma-clean-"):]
		os.chdir(DIR)
		if run(["git", "diff", "--quiet", "../src"]).returncode != 0:
			run(["git", "stash", "push", "../src"], check=True)
			git_stash_pop = True

	try:
		target_dir = BIN_DIR / benchmark / allocator
		os.makedirs(target_dir, exist_ok=True)
		os.chdir(DIR / benchmark)
		run(["cargo", "build", f"--target-dir={target_dir}", f"--features=allocator/{allocator_feature}", "--release"], check=True)
	finally:
		if git_stash_pop:
			os.chdir(DIR)
			run(["git", "stash", "pop"], check=True)

def run_bench(benchmark, args):
	benchmark = Path(benchmark)
	times = []
	rsss = []
	for allocator in ALLOCATORS:
		print(f"{benchmark}/{allocator}")

		target = BIN_DIR / benchmark / allocator / "release" / benchmark.name
		measurements = []
		for i in range(RUNS + WARMUP):
			print(i + 1, "...", sep="", end="", flush=True)
			measurements.append(measure(target, args))
			print(" Done")
		measurements = measurements[WARMUP:]
		(time, rss) = list(zip(*measurements))
		times.append((time, ) + stats(time))
		rsss.append((rss, ) + stats(rss))

		print(f"{benchmark}/{allocator}: {round(times[-1][1])}ms {round(rsss[-1][1])}kb")
		print()
	return (times, rsss)

for (benchmark, _args) in BENCHMARKS:
	for allocator in ALLOCATORS:
		compile_bench(benchmark, allocator)

shutil.rmtree(PLOT_DIR, ignore_errors=True)
for (benchmark, args) in BENCHMARKS:
	os.makedirs(PLOT_DIR / benchmark)
	(time, rss) = run_bench(benchmark, args)

	(times, time_mean, time_ci) = list(zip(*time))
	(rsss, rss_mean, rss_ci) = list(zip(*rss))

	x = list(range(0, len(ALLOCATORS)))

	with open(PLOT_DIR / benchmark / "time.json", "w") as f:
		json.dump({"allocators": ALLOCATORS, "mean": time_mean, "confidence_interval": time_ci, "values": times}, f)

	fig, ax = plt.subplots()
	ax.set_ylabel("Allocator")
	ax.set_ylabel("time (ms)")
	ax.set_xticks(x, labels=ALLOCATORS)
	ax.bar(x, time_mean, yerr=list(zip(*list(map(lambda x: (-x[0], x[1]), time_ci)))), capsize=5)
	plt.savefig(PLOT_DIR / benchmark / "time.bar.pdf")

	fig, ax = plt.subplots()
	ax.set_ylim(bottom=0)
	ax.set_ylabel("Allocator")
	ax.set_ylabel("time (ms)")
	ax.set_xticks(x, labels=ALLOCATORS)
	ax.violinplot(times, x, showmeans=True)
	ax.set_ylim(bottom=0, top=max(map(max, times))*1.05)
	plt.savefig(PLOT_DIR / benchmark / "time.violin.pdf")

	with open(PLOT_DIR / benchmark / "rss.json", "w") as f:
		json.dump({"allocators": ALLOCATORS, "mean": rss_mean, "confidence_interval": rss_ci, "values": rsss}, f)

	fig, ax = plt.subplots()
	ax.set_ylabel("Allocator")
	ax.set_ylabel("rss (MiB)")
	ax.set_xticks(x, labels=ALLOCATORS)
	ax.bar(x, rss_mean, yerr=list(zip(*list(map(lambda x: (-x[0], x[1]), rss_ci)))), capsize=5)
	plt.savefig(PLOT_DIR / benchmark / "rss.bar.pdf")

	fig, ax = plt.subplots()
	ax.set_ylim(bottom=0)
	ax.set_ylabel("Allocator")
	ax.set_ylabel("rss (MiB)")
	ax.set_xticks(x, labels=ALLOCATORS)
	ax.violinplot(rsss, x, showmeans=True)
	ax.set_ylim(bottom=0, top=max(map(max, rsss))*1.05)
	plt.savefig(PLOT_DIR / benchmark / "rss.violin.pdf")
