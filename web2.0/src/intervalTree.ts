class Node {
	start: number;
	end: number;
	max: number;
	left: Node | null = null;
	right: Node | null = null;

	constructor(start: number, end: number) {
		this.start = start;
		this.end = end;
		this.max = end;
	}
}

export class IntervalTree {
	root: Node | null = null;

	insert(start: number, end: number) {
		this.root = this.insertRec(this.root, start, end);
	}

	insertArray(arr: [number, number][]) {
		arr.forEach(([start, end]) => this.insert(start, end));
	}

	constructor(arr?: [number, number][]) {
		if (arr) this.insertArray(arr);
	}

	private insertRec(node: Node | null, start: number, end: number): Node {
		if (!node) return new Node(start, end);

		if (start < node.start) {
			node.left = this.insertRec(node.left, start, end);
		} else {
			node.right = this.insertRec(node.right, start, end);
		}

		node.max = Math.max(node.max, end);
		return node;
	}

	search(start: number, end?: number): Node | null {
		if (end === undefined) {
			end = start;
		}
		return this.searchRec(this.root, start, end);
	}

	private searchRec(node: Node | null, start: number, end: number): Node | null {
		if (!node) return null;

		if (node.start <= end && start <= node.end) return node;

		if (node.left && node.left.max >= start) {
			return this.searchRec(node.left, start, end);
		}

		return node.right ? this.searchRec(node.right, start, end) : null;
	}
}
