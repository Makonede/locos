use core::ptr::NonNull;

#[derive(Clone, Copy, Debug)]
pub struct FreeList {
    pub head: Option<NonNull<Node>>,
    pub len: usize,
}

unsafe impl Send for FreeList {}

impl Default for FreeList {
    fn default() -> Self {
        FreeList::new()
    }
}

impl FreeList {
    /// Creates a new empty free list.
    pub const fn new() -> Self {
        FreeList { head: None, len: 0 }
    }

    /// Pushes a frame onto the free list.
    pub const fn push(&mut self, ptr: NonNull<()>) {
        let node = ptr.cast::<Node>();
        unsafe {
            node.write(Node { next: self.head });
        }
        self.head = Some(node);
        self.len += 1;
    }

    /// Pops a frame from the free list.
    pub const fn pop(&mut self) -> Option<NonNull<()>> {
        if let Some(node) = self.head {
            self.head = unsafe { node.as_ref().next };
            self.len -= 1;
            Some(node.cast())
        } else {
            None
        }
    }

    // check if an element exists
    pub fn exists(&self, ptr: NonNull<()>) -> bool {
        let mut current = self.head;
        while let Some(node) = current {
            if node == ptr.cast::<Node>() {
                return true;
            }

            current = unsafe { node.as_ref().next };
        }
        false
    }

    /// Removes a block from the free list
    ///
    /// This method takes O(n) time
    pub fn remove(&mut self, ptr: NonNull<()>) {
        let mut current = self.head;
        let mut prev: Option<NonNull<Node>> = None;

        while let Some(node) = current {
            if node == ptr.cast::<Node>() {
                if let Some(mut prev) = prev {
                    unsafe {
                        prev.as_mut().next = node.as_ref().next;
                    }
                } else {
                    self.head = unsafe { node.as_ref().next };
                }
                self.len -= 1;
                return;
            }

            prev = current;
            current = unsafe { node.as_ref().next };
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// A node in the linked list of free frames.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub next: Option<NonNull<Node>>,
}

unsafe impl Send for Node {}

#[derive(Clone, Copy, Debug)]
pub struct DoubleFreeList {
    pub links: DoubleFreeListLink,
    pub len: usize,
}

unsafe impl Send for DoubleFreeList {}

impl Default for DoubleFreeList {
    fn default() -> Self {
        DoubleFreeList::new()
    }
}

impl DoubleFreeList {
    /// Creates a new empty double free list.
    pub const fn new() -> Self {
        DoubleFreeList {
            links: DoubleFreeListLink {
                next: None,
                prev: None,
            },
            len: 0,
        }
    }

    /// Pushes a node onto the double free list.
    pub const fn push(&mut self, mut node: NonNull<DoubleFreeListNode>, level_size: usize) {
        unsafe {
            if let Some(mut old_head) = self.links.next {
                old_head.as_mut().links.prev = Some(node);
            }
            node.as_mut().links.next = self.links.next;
            node.as_mut().links.prev = None;
            node.as_mut().level_size = Some(level_size);
            self.links.next = Some(node);
        }
        self.len += 1;
    }

    /// Pops the frontmost node of the double free list
    pub const fn pop(&mut self) -> Option<NonNull<DoubleFreeListNode>> {
        if let Some(mut node) = self.links.next {
            self.links.next = unsafe { node.as_mut().links.next };
            if let Some(mut next_node) = self.links.next {
                unsafe { next_node.as_mut().links.prev = None; }
            }
            self.len -= 1;
            Some(node)
        } else {
            None
        }
    }

    /// Returns the length of the double free list.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Checks if the double free list is empty.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Removes a specific node from the double free list.
    /// This is O(1) since we have direct access to the node.
    /// 
    /// # Safety
    /// The caller must ensure that:
    /// - The node pointer is valid and points to a properly initialized DoubleFreeListNode
    /// - The node is actually in this list
    /// - No other references to the node exist
    pub const unsafe fn remove(&mut self, mut node: NonNull<DoubleFreeListNode>) {
        let node_ref = unsafe { node.as_mut() };
        
        if let Some(mut prev) = node_ref.links.prev {
            unsafe { prev.as_mut() }.links.next = node_ref.links.next;
        } else {
            self.links.next = node_ref.links.next;
        }
        
        if let Some(mut next) = node_ref.links.next {
            unsafe { next.as_mut() }.links.prev = node_ref.links.prev;
        }

        node_ref.links.next = None;
        node_ref.links.prev = None;
        node_ref.level_size = None; // not part of any level anymore
        
        self.len -= 1;
    }

    /// Checks if a specific node exists in the list by checking that the
    /// level it belongs to matches the given level size.
    /// 
    /// # Safety
    /// The caller must ensure that the node pointer is valid and points to a properly 
    /// initialized DoubleFreeListNode.
    pub unsafe fn contains(&self, node: NonNull<DoubleFreeListNode>, level_size: usize) -> bool {
        let node_ref = unsafe { node.as_ref() };
        node_ref.level_size.is_some_and(|size| size == level_size)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DoubleFreeListLink {
    pub next: Option<NonNull<DoubleFreeListNode>>,
    pub prev: Option<NonNull<DoubleFreeListNode>>,
}

impl DoubleFreeListLink {
    pub const fn new(next: Option<NonNull<DoubleFreeListNode>>, prev: Option<NonNull<DoubleFreeListNode>>) -> Self {
        DoubleFreeListLink {
            next,
            prev,
        }
    }
}

unsafe impl Send for DoubleFreeListLink {}

#[derive(Clone, Copy, Debug)]
#[repr(align(32))]
pub struct DoubleFreeListNode {
    pub links: DoubleFreeListLink,
    /// Size of the level this node belongs to in pages.
    pub level_size: Option<usize>, 
}

impl DoubleFreeListNode {
    pub const fn new(links: DoubleFreeListLink, level_size: Option<usize>) -> Self {
        DoubleFreeListNode {
            links,
            level_size,
        }
    }
}