# Algorithmsts - Comprehensive Algorithms & Data Structures Library

[![npm version](https://badge.fury.io/js/%40mikbin80%2Falgorithmsts.svg)](https://badge.fury.io/js/%40mikbin80%2Falgorithmsts)
[![TypeScript](https://img.shields.io/badge/TypeScript-007ACC?style=flat&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A comprehensive, production-ready library of classic algorithms and data structures implemented in TypeScript. Designed for performance, type safety, and ease of use.

## Features

- 🚀 **High Performance**: Optimized implementations with O(log n), O(1) complexities where possible
- 🛡️ **Type Safe**: Full TypeScript support with generic implementations
- 📦 **Tree Shaking**: Modular imports for minimal bundle size
- 🔧 **Well Tested**: Comprehensive test suite with 95%+ coverage
- 📚 **Well Documented**: Extensive JSDoc documentation with complexity analysis
- 🔄 **Backward Compatible**: Legacy API support with deprecation warnings

## Installation

```bash
npm install @mikbin80/algorithmsts
```

## Quick Start

### Modern Modular Import (Recommended)

```typescript
import { BinarySearch, LinkedList, AdjacencyListGraph } from '@mikbin80/algorithmsts';

// Binary search
const result = BinarySearch.search([1, 2, 3, 4, 5], 3); // returns index 2

// Linked list
const list = new LinkedList<number>();
list.add(1);
list.add(2);

// Graph operations
const graph = new AdjacencyListGraph<string>(true); // directed graph
graph.addVertex('A');
graph.addVertex('B');
graph.addEdge('A', 'B');
```

### Legacy Import (Deprecated)

```typescript
import algorithmsts from '@mikbin80/algorithmsts';

// Still works but deprecated
const result = algorithmsts.binarySearch.search([1, 2, 3], 2);
```

## Library Structure

The library is organized into modular components:

- **Core**: Fundamental interfaces, abstract classes, and utilities
- **Data Structures**: Efficient data organization and storage
- **Algorithms**: Classic algorithm implementations
- **Graph Theory**: Graph data structures and algorithms
- **Performance**: Monitoring and benchmarking tools

## Data Structures

### Linear Data Structures

#### Linked List
Doubly-linked list implementation with O(1) insertion/deletion.

```typescript
import { LinkedList } from '@mikbin80/algorithmsts/data-structures';

const list = new LinkedList<number>();
list.add(1);
list.add(2);
list.add(3);

console.log(list.toArray()); // [1, 2, 3]
```

### Code Examples

#### Queue
A queue is a linear data structure that follows the First-In-First-Out (FIFO) principle.

```typescript
import { Queue } from '@mikbin80/algorithmsts/data-structures';

const queue = new Queue<number>();
queue.enqueue(1);
queue.enqueue(2);
queue.enqueue(3);

console.log(queue.dequeue()); // 1
console.log(queue.peek()); // 2
```

#### Stack
A stack is a linear data structure that follows the Last-In-First-Out (LIFO) principle.

```typescript
import { Stack } from '@mikbin80/algorithmsts/data-structures';

const stack = new Stack<number>();
stack.push(1);
stack.push(2);
stack.push(3);

console.log(stack.pop()); // 3
console.log(stack.peek()); // 2
```

#### Hash Map
A hash map is a data structure that implements an associative array abstract data type, a structure that can map keys to values.

```typescript
import { HashMap } from '@mikbin80/algorithmsts/data-structures';

const map = new HashMap<string, number>();
map.set("a", 1);
map.set("b", 2);
map.set("c", 3);

console.log(map.get("b")); // 2
console.log(map.has("c")); // true
map.delete("a");
```

#### Priority Queue
A priority queue is a data structure that allows adding elements with a priority and retrieving the element with the highest priority.

```typescript
import { PriorityQueue } from '@mikbin80/algorithmsts/data-structures';

const pq = new PriorityQueue<number>((a, b) => b - a);
pq.enqueue(3);
pq.enqueue(1);
pq.enqueue(4);

console.log(pq.dequeue()); // 4
console.log(pq.peek()); // 3
```

#### Disjoint Set
A disjoint set is a data structure that keeps track of a set of elements partitioned into a number of disjoint (non-overlapping) subsets.

```typescript
import { UnionFind } from '@mikbin80/algorithmsts/data-structures';

const uf = new UnionFind(5);
uf.union(0, 1);
uf.union(2, 3);
uf.union(0, 4);

console.log(uf.connected(1, 4)); // true
console.log(uf.connected(1, 2)); // false
```

### Sorting Algorithms

#### Counting Sort
Counting sort is a sorting algorithm that sorts the elements of an array by counting the number of occurrences of each unique element in the array.

```typescript
import { CountingSort } from '@mikbin80/algorithmsts/algorithms';

const countingSort = new CountingSort();
const sortedArray = countingSort.execute({ array: [4, 2, 2, 8, 3, 3, 1] });
console.log(sortedArray); // { sortedArray: [1, 2, 2, 3, 3, 4, 8], result: [1, 2, 2, 3, 3, 4, 8] }
```

#### Radix Sort
Radix sort is a non-comparative sorting algorithm that sorts data with integer keys by grouping keys by the individual digits which share the same significant position and value.

```typescript
import { RadixSort } from '@mikbin80/algorithmsts/algorithms';

const radixSort = new RadixSort();
const sortedArray = radixSort.execute({ array: [170, 45, 75, 90, 802, 24, 2, 66] });
console.log(sortedArray); // { sortedArray: [2, 24, 45, 66, 75, 90, 170, 802], result: [2, 24, 45, 66, 75, 90, 170, 802] }
```

### Probabilistic Data Structures

#### Skip List
Probabilistic alternative to balanced trees with O(log n) operations.

```typescript
import { SkipList } from '@mikbin80/algorithmsts/data-structures';

const skipList = new SkipList<number>(10);
skipList.insert(15);
skipList.insert(5);
skipList.insert(25);

const found = skipList.find(15); // returns the node
```

### Tree-Based Data Structures

#### Binary Search Tree (BST)
Self-balancing binary search tree.

```typescript
import { BinarySearchTree } from '@mikbin80/algorithmsts/data-structures';

const bst = new BinarySearchTree<number>();
bst.insert(50);
bst.insert(30);
bst.insert(70);

console.log(bst.contains(30)); // true
```

#### AVL Tree
Height-balanced binary search tree.

```typescript
import { AVLTree } from '@mikbin80/algorithmsts/data-structures';

const avl = new AVLTree<number>();
avl.insert(10);
avl.insert(20);
avl.insert(30); // auto-balances
```

#### Segment Tree
Versatile data structure for range queries and updates.

```typescript
import { SegmentTree } from '@mikbin80/algorithmsts/data-structures';

// Range sum queries
const array = [1, 3, 5, 7, 9, 11];
const segmentTree = new SegmentTree<number, {sum: number}>(array, ...);

// Query sum of range [1, 4]
const sum = segmentTree.query(1, 4); // {sum: 24}
```

#### B-Tree
Self-balancing tree data structure that maintains sorted data.

```typescript
import { BTree } from '@mikbin80/algorithmsts/data-structures';

const btree = new BTree<number>(3); // minimum degree 3
btree.insert(10);
btree.insert(20);
btree.insert(5);
```

#### Red-Black Tree
Balanced binary search tree with guaranteed O(log n) operations.

```typescript
import { RedBlackTree } from '@mikbin80/algorithmsts/data-structures';

const rbt = new RedBlackTree<number>();
rbt.insert(10);
rbt.insert(20);
rbt.insert(5);
```

#### Fenwick Tree (Binary Indexed Tree)
Data structure for prefix sum queries and point updates.

```typescript
import { FenwickTree } from '@mikbin80/algorithmsts/data-structures';

const fenwick = new FenwickTree([1, 2, 3, 4, 5]);
fenwick.update(2, 10); // update index 2 (0-based) to 10
const sum = fenwick.query(4); // sum of first 5 elements
```

### Heap Data Structures

#### Binary Heap
Complete binary tree with heap property.

```typescript
import { BinaryHeap } from '@mikbin80/algorithmsts/data-structures';

const minHeap = new BinaryHeap<number>();
minHeap.insert(5);
minHeap.insert(3);
minHeap.insert(8);
const min = minHeap.extractMin(); // 3
```

### String Data Structures

#### Trie
Prefix tree for efficient string operations.

```typescript
import { Trie } from '@mikbin80/algorithmsts/data-structures';

const trie = new Trie<string, number>();
trie.insert("apple", 1);
trie.insert("apply", 2);

console.log(trie.get("apple")); // 1
console.log(trie.startsWith("app")); // true
```

#### Suffix Tree
Advanced string processing using Ukkonen's algorithm.

```typescript
import { SuffixTree } from '@mikbin80/algorithmsts/data-structures';

const suffixTree = new SuffixTree("banana");
const index = suffixTree.findSubstring("ana"); // 1
```

## Graph Theory

### Graph Structures

#### Adjacency List Graph
Memory-efficient representation for sparse graphs.

```typescript
import { AdjacencyListGraph } from '@mikbin80/algorithmsts/graphs';

// Directed graph
const directedGraph = new AdjacencyListGraph<string>(true);
directedGraph.addVertex('A');
directedGraph.addVertex('B');
directedGraph.addEdge('A', 'B');

// Undirected graph
const undirectedGraph = new AdjacencyListGraph<string>(false);
undirectedGraph.addEdge('A', 'B'); // adds both A->B and B->A
```

#### Adjacency Matrix Graph
Efficient representation for dense graphs.

```typescript
import { AdjacencyMatrixGraph } from '@mikbin80/algorithmsts/graphs';

const matrixGraph = new AdjacencyMatrixGraph<number>();
matrixGraph.addVertex(1);
matrixGraph.addVertex(2);
matrixGraph.addEdge(1, 2);

const neighbors = matrixGraph.getNeighbors(1); // [2]
```

### Graph Algorithms

#### Traversal

```typescript
import { BreadthFirstSearch, DepthFirstSearch } from '@mikbin80/algorithmsts/graphs';

const bfs = new BreadthFirstSearch(graph);
const traversal = bfs.traverse('A');

const dfs = new DepthFirstSearch(graph);
const path = dfs.findPath('A', 'D');
```

#### Shortest Path

```typescript
import { DijkstraAlgorithm, AStarAlgorithm } from '@mikbin80/algorithmsts/graphs';

const dijkstra = new DijkstraAlgorithm(weightedGraph);
const shortestPath = dijkstra.findShortestPath('A', 'Z');

const aStar = new AStarAlgorithm(weightedGraph, heuristic);
const optimalPath = aStar.findShortestPath('A', 'Z');
```

#### Topological Sort

```typescript
import { TopologicalSort } from '@mikbin80/algorithmsts/graphs';

const topoSort = new TopologicalSort(directedGraph);
const order = topoSort.sort(); // returns topological ordering
```

#### Cycle Detection

```typescript
import { CycleDetection } from '@mikbin80/algorithmsts/graphs';

const cycleDetector = new CycleDetection(graph);
const hasCycle = cycleDetector.hasCycle();
```

## Algorithms

### Search Algorithms

#### Binary Search
O(log n) search in sorted arrays.

```typescript
import { BinarySearch } from '@mikbin80/algorithmsts/algorithms';

const sorted = [1, 3, 5, 7, 9, 11, 13, 15];
const index = BinarySearch.search(sorted, 7); // returns 3
const closest = BinarySearch.closestSearch(sorted, 8); // returns index of 7
```

### Sorting Algorithms

#### Counting Sort
Linear time sorting for integers with limited range.

```typescript
import { CountingSort } from '@mikbin80/algorithmsts/algorithms';

const numbers = [4, 2, 2, 8, 3, 3, 1];
const sorted = CountingSort.sort(numbers, 0, 8); // [1, 2, 2, 3, 3, 4, 8]
```

#### Radix Sort
Efficient sorting for integers and strings.

```typescript
import { RadixSortNumbers, RadixSortStrings } from '@mikbin80/algorithmsts/algorithms';

const numbers = [170, 45, 75, 90, 802, 24, 2, 66];
RadixSortNumbers.sort(numbers); // [2, 24, 45, 66, 75, 90, 170, 802]

const strings = ["apple", "banana", "cherry", "date"];
RadixSortStrings.sort(strings); // ["apple", "banana", "cherry", "date"]
```

### String Algorithms

#### String Similarity

```typescript
import { NgramSimilarity, JaroDistance, JaroWinklerDistance, LevenshteinDistance } from '@mikbin80/algorithmsts/algorithms';

const similarity = NgramSimilarity.calculate("night", "nacht", 2);
const jaro = JaroDistance.calculate("martha", "marhta");
const jaroWinkler = JaroWinklerDistance.calculate("martha", "marhta");
const levenshtein = LevenshteinDistance.calculate("kitten", "sitting");
```

### Range Query Algorithms

#### Sparse Table
O(1) range minimum/maximum queries after O(n log n) preprocessing.

```typescript
import { SparseTable } from '@mikbin80/algorithmsts/algorithms';

const array = [1, 3, 2, 7, 9, 11];
const sparseTable = new SparseTable(array, Math.min);
const min = sparseTable.query(1, 4); // minimum of [3, 2, 7, 9] = 2
```

### Performance Utilities

#### Algorithm Comparison

```typescript
import { BenchmarkRunner } from '@mikbin80/algorithmsts/performance';
import { AlgorithmSelector } from '@mikbin80/algorithmsts/algorithms';

const result = BenchmarkRunner.compareAlgorithms(algo1, algo2, testData);

const selector = new AlgorithmSelector();
const bestAlgorithm = selector.select(algorithms, criteria, data);
```

## Core Utilities

### Performance Monitoring

```typescript
import { PerformanceMonitor } from '@mikbin80/algorithmsts/performance';

const time = PerformanceMonitor.measureExecutionTime(() => {
  // ... perform operation ...
});
console.log(`Execution time: ${time}ms`);
```

### Validation

```typescript
import { Validator, ArgumentError, DataStructureError } from '@mikbin80/algorithmsts/core';

Validator.notNull(value, 'parameterName');
Validator.inRange(number, 0, 100, 'age');
Validator.notEmpty(array, 'items');
```

## Type Definitions

```typescript
import { 
  AlgorithmComparisonResult,
  AlgorithmPerformanceMetrics,
  SelectionCriteria,
  IGraph,
  IWeightedGraph,
  ITraversalAlgorithm
} from '@mikbin80/algorithmsts/types';
```

## Compatibility Layer

The library includes a compatibility layer for backward compatibility:

```typescript
import { LinkedList as LegacyLinkedList } from '@mikbin80/algorithmsts/compatibility';

// Legacy API with deprecation warnings
const list = new LegacyLinkedList<number>();
```

## Performance Characteristics

| Data Structure | Insert | Delete | Search | Memory |
|---|---|---|---|---|
| LinkedList | O(1) | O(1) | O(n) | O(n) |
| SkipList | O(log n) | O(log n) | O(log n) | O(n) |
| BinarySearchTree | O(log n)* | O(log n)* | O(log n)* | O(n) |
| AVLTree | O(log n) | O(log n) | O(log n) | O(n) |
| BTree | O(log n) | O(log n) | O(log n) | O(n) |
| RedBlackTree | O(log n) | O(log n) | O(log n) | O(n) |
| BinaryHeap | O(log n) | O(log n)** | O(n) | O(n) |
| Trie | O(m)*** | O(m)*** | O(m)*** | O(n*m) |
| SegmentTree | O(log n) | O(log n) | O(log n) | O(n) |

*Average case, **Extract min/max, ***m = string length

## Contributing

Contributions are welcome! Please read our contributing guidelines and code of conduct.

### Development Setup

```bash
git clone https://github.com/mikbin80/algorithmsts.git
cd algorithmsts
npm install
npm run build
npm run test
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history.

## Support

- 📧 Email: support@mikbin80.dev
- 🐛 Issues: [GitHub Issues](https://github.com/mikbin80/algorithmsts/issues)
- 💬 Discussions: [GitHub Discussions](https://github.com/mikbin80/algorithmsts/discussions)

---

**Algorithmsts** - Making algorithms accessible, performant, and type-safe for TypeScript developers.
