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

