#!/usr/bin/env python3
"""
=============================================================================
 TEST PROGRAM  490 Lines of Code
 A collection of algorithms, data structures, and utilities for learning.
=============================================================================
"""

import math
import random
import json
import sys
import time
from collections import deque, Counter
from typing import List, Dict, Optional, Tuple, Any
from dataclasses import dataclass
from enum import Enum, auto


# ---------------------------------------------------------------------------
# 1. Utility Functions
# ---------------------------------------------------------------------------

def clamp(value: float, low: float, high: float) -> float:
    """Clamp a value between low and high."""
    return max(low, min(value, high))


def lerp(a: float, b: float, t: float) -> float:
    """Linear interpolation between a and b by factor t."""
    return a + (b - a) * t


def factorial(n: int) -> int:
    """Compute factorial iteratively."""
    if n < 0:
        raise ValueError("Factorial is not defined for negative numbers.")
    result = 1
    for i in range(2, n + 1):
        result *= i
    return result


def is_prime(n: int) -> bool:
    """Check if a number is prime."""
    if n < 2:
        return False
    if n < 4:
        return True
    if n % 2 == 0 or n % 3 == 0:
        return False
    i = 5
    while i * i <= n:
        if n % i == 0 or n % (i + 2) == 0:
            return False
        i += 6
    return True


def gcd(a: int, b: int) -> int:
    """Greatest common divisor using Euclidean algorithm."""
    while b:
        a, b = b, a % b
    return a


def lcm(a: int, b: int) -> int:
    """Least common multiple."""
    return abs(a * b) // gcd(a, b)


# ---------------------------------------------------------------------------
# 2. Sorting Algorithms
# ---------------------------------------------------------------------------

def bubble_sort(arr: List[int]) -> List[int]:
    """Bubble sort  O(n) time."""
    a = arr[:]
    n = len(a)
    for i in range(n):
        swapped = False
        for j in range(0, n - i - 1):
            if a[j] > a[j + 1]:
                a[j], a[j + 1] = a[j + 1], a[j]
                swapped = True
        if not swapped:
            break
    return a


def insertion_sort(arr: List[int]) -> List[int]:
    """Insertion sort  O(n) time, good for nearly-sorted data."""
    a = arr[:]
    for i in range(1, len(a)):
        key = a[i]
        j = i - 1
        while j >= 0 and a[j] > key:
            a[j + 1] = a[j]
            j -= 1
        a[j + 1] = key
    return a


def merge_sort(arr: List[int]) -> List[int]:
    """Merge sort  O(n log n) time, stable."""

    def merge(left, right):
        result = []
        i = j = 0
        while i < len(left) and j < len(right):
            if left[i] <= right[j]:
                result.append(left[i])
                i += 1
            else:
                result.append(right[j])
                j += 1
        result.extend(left[i:])
        result.extend(right[j:])
        return result

    if len(arr) <= 1:
        return arr[:]

    mid = len(arr) // 2
    left = merge_sort(arr[:mid])
    right = merge_sort(arr[mid:])
    return merge(left, right)


# ---------------------------------------------------------------------------
# 3. Search Algorithms
# ---------------------------------------------------------------------------

def linear_search(arr: List[int], target: int) -> Optional[int]:
    """Find target index via linear search. Returns None if not found."""
    for i, val in enumerate(arr):
        if val == target:
            return i
    return None


def binary_search(arr: List[int], target: int) -> Optional[int]:
    """Binary search on a sorted list. Returns index or None."""
    low, high = 0, len(arr) - 1
    while low <= high:
        mid = (low + high) // 2
        if arr[mid] == target:
            return mid
        elif arr[mid] < target:
            low = mid + 1
        else:
            high = mid - 1
    return None


# ---------------------------------------------------------------------------
# 4. Data Structures
# ---------------------------------------------------------------------------

class Stack:
    """LIFO stack implemented with a list."""

    def __init__(self) -> None:
        self._items: List[Any] = []

    def push(self, item: Any) -> None:
        self._items.append(item)

    def pop(self) -> Any:
        if self.is_empty:
            raise IndexError("Pop from empty stack.")
        return self._items.pop()

    def peek(self) -> Any:
        if self.is_empty:
            raise IndexError("Peek from empty stack.")
        return self._items[-1]

    @property
    def is_empty(self) -> bool:
        return len(self._items) == 0

    def __len__(self) -> int:
        return len(self._items)

    def __repr__(self) -> str:
        return f"Stack({self._items})"


class Queue:
    """FIFO queue using collections.deque."""

    def __init__(self) -> None:
        self._items: deque = deque()

    def enqueue(self, item: Any) -> None:
        self._items.append(item)

    def dequeue(self) -> Any:
        if self.is_empty:
            raise IndexError("Dequeue from empty queue.")
        return self._items.popleft()

    def front(self) -> Any:
        if self.is_empty:
            raise IndexError("Front from empty queue.")
        return self._items[0]

    @property
    def is_empty(self) -> bool:
        return len(self._items) == 0

    def __len__(self) -> int:
        return len(self._items)

    def __repr__(self) -> str:
        return f"Queue({list(self._items)})"


# ---------------------------------------------------------------------------
# 5. Graph Algorithms
# ---------------------------------------------------------------------------

class Graph:
    """Undirected graph using adjacency list."""

    def __init__(self) -> None:
        self._adj: Dict[int, List[int]] = {}

    def add_vertex(self, v: int) -> None:
        if v not in self._adj:
            self._adj[v] = []

    def add_edge(self, u: int, v: int) -> None:
        self.add_vertex(u)
        self.add_vertex(v)
        self._adj[u].append(v)
        self._adj[v].append(u)

    def bfs(self, start: int) -> List[int]:
        """Breadth-first traversal from start node."""
        visited: set = set()
        order: List[int] = []
        q = Queue()
        q.enqueue(start)
        visited.add(start)
        while q:
            node = q.dequeue()
            order.append(node)
            for neighbor in self._adj.get(node, []):
                if neighbor not in visited:
                    visited.add(neighbor)
                    q.enqueue(neighbor)
        return order

    def dfs(self, start: int) -> List[int]:
        """Depth-first traversal from start node."""
        visited: set = set()
        order: List[int] = []
        stack = Stack()
        stack.push(start)
        while stack:
            node = stack.pop()
            if node not in visited:
                visited.add(node)
                order.append(node)
                for neighbor in reversed(self._adj.get(node, [])):
                    if neighbor not in visited:
                        stack.push(neighbor)
        return order

    def __repr__(self) -> str:
        return f"Graph({self._adj})"


# ---------------------------------------------------------------------------
# 6. Simple Pathfinding (BFS on grid)
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class Point:
    """2D point with integer coordinates."""
    x: int
    y: int

    def neighbors(self) -> List["Point"]:
        return [
            Point(self.x + 1, self.y),
            Point(self.x - 1, self.y),
            Point(self.x, self.y + 1),
            Point(self.x, self.y - 1),
        ]

    def __repr__(self) -> str:
        return f"({self.x},{self.y})"


def shortest_path(
    start: Point, goal: Point, obstacles: set
) -> Optional[List[Point]]:
    """BFS shortest path on a 2D grid avoiding obstacles."""
    if start == goal:
        return [start]

    q = deque()
    q.append(start)
    came_from = {start: None}

    while q:
        current = q.popleft()
        for nb in current.neighbors():
            if nb in obstacles:
                continue
            if nb not in came_from:
                came_from[nb] = current
                if nb == goal:
                    path = []
                    while nb is not None:
                        path.append(nb)
                        nb = came_from[nb]
                    path.reverse()
                    return path
                q.append(nb)
    return None


# ---------------------------------------------------------------------------
# 7. Temperature Converter & Unit Tests
# ---------------------------------------------------------------------------

class TemperatureScale(Enum):
    CELSIUS = auto()
    FAHRENHEIT = auto()
    KELVIN = auto()


def convert_temperature(
    value: float, from_scale: TemperatureScale, to_scale: TemperatureScale
) -> float:
    """Convert between Celsius, Fahrenheit, and Kelvin."""
    if from_scale == to_scale:
        return value

    if from_scale == TemperatureScale.CELSIUS:
        if to_scale == TemperatureScale.FAHRENHEIT:
            return value * 9.0 / 5.0 + 32.0
        elif to_scale == TemperatureScale.KELVIN:
            return value + 273.15
    elif from_scale == TemperatureScale.FAHRENHEIT:
        celsius = (value - 32.0) * 5.0 / 9.0
        if to_scale == TemperatureScale.CELSIUS:
            return celsius
        elif to_scale == TemperatureScale.KELVIN:
            return celsius + 273.15
    elif from_scale == TemperatureScale.KELVIN:
        if to_scale == TemperatureScale.CELSIUS:
            return value - 273.15
        elif to_scale == TemperatureScale.FAHRENHEIT:
            return (value - 273.15) * 9.0 / 5.0 + 32.0

    raise ValueError(f"Cannot convert {from_scale} to {to_scale}.")


# ---------------------------------------------------------------------------
# 8. Self-Test Runner
# ---------------------------------------------------------------------------

def run_tests() -> int:
    """Run all internal tests. Returns number of failures."""
    failures = 0

    def check(condition: bool, msg: str) -> None:
        nonlocal failures
        if not condition:
            print(f"  FAIL: {msg}")
            failures += 1
        else:
            print(f"  OK:   {msg}")

    print("\n=== Running Tests ===\n")

    # --- Utility tests ---
    check(clamp(10, 0, 5) == 5, "clamp upper")
    check(clamp(-1, 0, 5) == 0, "clamp lower")
    check(clamp(3, 0, 5) == 3, "clamp middle")
    check(lerp(0, 10, 0.5) == 5.0, "lerp")
    check(factorial(5) == 120, "factorial 5")
    check(factorial(0) == 1, "factorial 0")
    check(is_prime(2), "prime 2")
    check(not is_prime(4), "not prime 4")
    check(is_prime(17), "prime 17")
    check(gcd(12, 8) == 4, "gcd 12,8")
    check(lcm(4, 6) == 12, "lcm 4,6")

    # --- Sorting tests ---
    unsorted = [3, 1, 4, 1, 5, 9, 2, 6, 5]
    check(bubble_sort(unsorted) == sorted(unsorted), "bubble sort")
    check(insertion_sort(unsorted) == sorted(unsorted), "insertion sort")
    check(merge_sort(unsorted) == sorted(unsorted), "merge sort")

    # --- Search tests ---
    sorted_list = [1, 3, 5, 7, 9, 11, 13]
    check(binary_search(sorted_list, 7) == 3, "binary search found")
    check(binary_search(sorted_list, 2) is None, "binary search not found")
    check(linear_search(sorted_list, 7) == 3, "linear search found")

    # --- Stack tests ---
    s = Stack()
    s.push(10)
    s.push(20)
    check(s.pop() == 20, "stack LIFO")
    check(s.pop() == 10, "stack LIFO 2")
    check(s.is_empty, "stack empty after pops")

    # --- Queue tests ---
    q = Queue()
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    check(q.dequeue() == 1, "queue FIFO")
    check(q.dequeue() == 2, "queue FIFO 2")

    # --- Graph tests ---
    g = Graph()
    g.add_edge(0, 1)
    g.add_edge(0, 2)
    g.add_edge(1, 2)
    g.add_edge(2, 3)
    bfs_result = g.bfs(0)
    check(bfs_result == [0, 1, 2, 3], "graph BFS")
    dfs_result = g.dfs(0)
    check(dfs_result == [0, 1, 2, 3], "graph DFS")

    # --- Pathfinding tests ---
    obstacles = {Point(0, 1), Point(1, 1), Point(2, 1)}
    path = shortest_path(Point(0, 0), Point(2, 2), obstacles)
    check(path is not None, "pathfinding finds a route")

    # --- Temperature tests ---
    check(
        abs(convert_temperature(0, TemperatureScale.CELSIUS, TemperatureScale.FAHRENHEIT) - 32.0) < 0.01,
        "0C -> 32F",
    )
    check(
        abs(convert_temperature(100, TemperatureScale.CELSIUS, TemperatureScale.KELVIN) - 373.15) < 0.01,
        "100C -> 373.15 K",
    )

    print(f"\n--- {failures} failure(s) ---\n")
    return failures


# ---------------------------------------------------------------------------
# 9. Main Entry Point
# ---------------------------------------------------------------------------

def main() -> int:
    """Entry point: run tests and show some demos."""
    print("=" * 60)
    print("  TEST PROGRAM  490 Lines of Code")
    print("=" * 60)

    fail_count = run_tests()

    print("\n=== Demo: Sorting === ")
    sample = [random.randint(1, 100) for _ in range(10)]
    print(f"  Original: {sample}")
    print(f"  Sorted:   {merge_sort(sample)}")

    print("\n=== Demo: Temp Conversion === ")
    c = 25.0
    f = convert_temperature(c, TemperatureScale.CELSIUS, TemperatureScale.FAHRENHEIT)
    k = convert_temperature(c, TemperatureScale.CELSIUS, TemperatureScale.KELVIN)
    print(f"  {c}C = {f:.1f}F = {k:.2f} K")

    print("\n=== Demo: Pathfinding === ")
    obstacles = {
        Point(1, 0), Point(1, 1), Point(1, 2),
        Point(3, 0), Point(3, 1), Point(3, 2),
    }
    start, goal = Point(0, 0), Point(4, 0)
    path = shortest_path(start, goal, obstacles)
    print(f"  Start: {start}  Goal: {goal}")
    print(f"  Shortest path: {path}")

    return 0 if fail_count == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
