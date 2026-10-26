// What the middle pane's heading is handed (docs/design.md, 「The screen」): the title naming the brain and the
// view, every choice standing, the lines saying what the view cannot honour, and how every choice is cleared.

// One choice as the heading shows it: which, its value in words, whether the view honours it, and how it is
// taken back.
export interface HeadingChip {
  choice: string;
  what: string;
  value: string;
  honoured: boolean;
  clear: () => void;
}

export interface Heading {
  title: string;
  chips: HeadingChip[];
  notes: string[];
  clearAll: () => void;
}
