"use client";

import React, { useState, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';

export default function CategorySelector({ value, onChange, categories = [] }) {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');
  const wrapperRef = useRef(null);
  const inputRef = useRef(null);

  useEffect(() => {
    function handleClickOutside(event) {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [wrapperRef]);

  // When opening, reset search to empty or current value if we want to filter by current
  const handleOpen = () => {
    setSearch('');
    setIsOpen(true);
    setTimeout(() => {
      if (inputRef.current) inputRef.current.focus();
    }, 50);
  };

  const handleSelect = (cat) => {
    onChange(cat);
    setIsOpen(false);
  };

  const filteredCategories = categories.filter(cat => 
    cat.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="relative" ref={wrapperRef}>
      {/* Display Button / Input Trigger */}
      <div 
        onClick={handleOpen}
        className="bg-black border border-white/10 p-2 rounded text-xs text-white/80 font-mono w-64 hover:border-accent cursor-pointer flex justify-between items-center transition-colors"
      >
        <span className="truncate">{value || "Select Category..."}</span>
        <span className="text-white/30 text-[10px]">▼</span>
      </div>

      {/* Popover Menu */}
      <AnimatePresence>
        {isOpen && (
          <motion.div
            initial={{ opacity: 0, y: -5 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -5 }}
            transition={{ duration: 0.15 }}
            className="absolute z-50 top-full mt-1 right-0 w-80 bg-zinc-950 border border-white/20 rounded shadow-2xl overflow-hidden"
          >
            {/* Search Input */}
            <div className="p-2 border-b border-white/10 bg-zinc-900/50">
              <input
                ref={inputRef}
                type="text"
                placeholder="Search categories..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-full bg-black border border-white/10 p-2 text-xs text-white font-mono rounded outline-none focus:border-accent transition-colors"
              />
            </div>

            {/* List */}
            <div className="max-h-64 overflow-y-auto custom-scrollbar">
              {filteredCategories.length === 0 ? (
                <div className="p-4 text-center text-[10px] text-white/40 uppercase tracking-widest font-mono">
                  No matching categories
                </div>
              ) : (
                filteredCategories.map(cat => (
                  <div
                    key={cat}
                    onClick={() => handleSelect(cat)}
                    className="p-2 px-3 text-xs font-mono text-white/70 hover:bg-accent/20 hover:text-white cursor-pointer transition-colors border-b border-white/5 last:border-0 truncate"
                    title={cat}
                  >
                    {/* Visual trick: slightly indent sub-categories based on colon count */}
                    <span style={{ paddingLeft: `${(cat.split(':').length - 1) * 6}px` }}>
                      {cat.split(':').pop()} <span className="text-[9px] text-white/20 ml-2">{cat}</span>
                    </span>
                  </div>
                ))
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
