# Calendar View Feature - Visual Guide

## Calendar View Structure

### Weekly View Layout

```
┌─────────────────────────────────────────────────────────────┐
│  ← Calendar                          [Week] [Month]  🔄     │
│    Next 7 days for [Child Name]                             │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  📅 Monday, January 27 (Today)                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ✓ Chores                                            │   │
│  │  ┌───────────────────────────────────────────────┐  │   │
│  │  │ ✅ Make Bed                                    │  │   │
│  │  │    Wake up and make your bed                   │  │   │
│  │  │    [AM] [10 pts]                               │  │   │
│  │  └───────────────────────────────────────────────┘  │   │
│  │  ┌───────────────────────────────────────────────┐  │   │
│  │  │ ⭕ Brush Teeth                                  │  │   │
│  │  │    Brush teeth after breakfast                 │  │   │
│  │  │    [AM] [5 pts]                                │  │   │
│  │  └───────────────────────────────────────────────┘  │   │
│  │                                                      │   │
│  │  🗓️ Events                                          │   │
│  │  ┌───────────────────────────────────────────────┐  │   │
│  │  │ 🕐 3:00 PM  Soccer Practice                    │  │   │
│  │  │             📍 City Park                        │  │   │
│  │  └───────────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                               │
│  📅 Tuesday, January 28                                      │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ✓ Chores                                            │   │
│  │  ┌───────────────────────────────────────────────┐  │   │
│  │  │ ⭕ Clean Room                                   │  │   │
│  │  │    Tidy up bedroom                             │  │   │
│  │  │    [20 pts]                                    │  │   │
│  │  └───────────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                               │
│  📅 Wednesday, January 29                                    │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  No chores or events                                 │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                               │
│  ... (continues for 7 days)                                  │
└─────────────────────────────────────────────────────────────┘
```

### Monthly View Layout

```
┌─────────────────────────────────────────────────────────────┐
│  ← Calendar                          [Week] [Month]  🔄     │
│    This month for [Child Name]                              │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  📅 Mon, Jan 27 (Today)                 [2/3 chores] [1 events] │
│                                                               │
│  📅 Tue, Jan 28                         [1/1 chores] [0 events] │
│                                                               │
│  📅 Wed, Jan 29                         [0/0 chores] [0 events] │
│                                                               │
│  📅 Thu, Jan 30                         [3/3 chores] [1 events] │
│                                                               │
│  📅 Fri, Jan 31                         [2/4 chores] [0 events] │
│                                                               │
│  ... (continues for all days in month)                       │
└─────────────────────────────────────────────────────────────┘
```

## Color Coding

### Chores
- 🟢 **Completed Chores**: Green background with checkmark icon
  - Indicates the chore has been completed
  - May show strikethrough text
  
- ⚪ **Pending Chores**: Gray/muted background with circle icon
  - Chore is scheduled but not yet completed
  - Ready to be done

### Events
- 🔵 **Calendar Events**: Blue background
  - External ICS calendar events
  - Displays time, title, location, description

### Special Indicators
- 🌟 **Today**: Primary border color highlighting current day
- 🕐 **Time Display**: Shows when event occurs or chore time window
- 🏆 **Points Badge**: Shows point value for completing chore
- 🌅 **Time of Day**: AM/PM badges for time-specific chores

## Data Flow

```
App.tsx
  │
  ├─→ chores: Array of all chores
  ├─→ assignments: Child-chore mappings
  ├─→ completions: Completion records
  ├─→ categories: Point categories
  │
  ▼
CalendarView.tsx
  │
  ├─→ Filter assignments by child
  ├─→ Create chores map for lookup
  ├─→ Generate date range (7 days or month)
  │
  ▼
For Each Date:
  │
  ├─→ getChoresForDate(date)
  │   ├─→ Check if assignment is active (date range)
  │   ├─→ Check if scheduled for this date
  │   ├─→ Check completion status
  │   └─→ Return array of chores with metadata
  │
  ├─→ Fetch ICS events for date
  │
  └─→ Render day card with chores and events
```

## Helper Functions Usage

### Checking if a Chore Appears on a Date

```typescript
// 1. Check if assignment is within date range
if (!isChoreActiveForDate(assignment, date)) return

// 2. Check if chore is scheduled for this day of week
if (!isChoreActiveOnDate(assignment, date)) return

// 3. Check if completed on this date
const completed = isChoreCompletedOnDate(
  completions, 
  chore.id, 
  child.id, 
  date, 
  timeOfDay
)
```

### Repeat Pattern Example

```typescript
// Weekly chore every Monday and Wednesday
assignment = {
  repeatPattern: {
    interval: 1,
    unit: 'weeks',
    specificDays: ['monday', 'wednesday']
  }
}

// Check specific date
isChoreActiveOnDate(assignment, new Date('2026-01-27')) // Monday → true
isChoreActiveOnDate(assignment, new Date('2026-01-28')) // Tuesday → false
isChoreActiveOnDate(assignment, new Date('2026-01-29')) // Wednesday → true
```

### Bi-Weekly Pattern Example

```typescript
// Every other Monday starting from anchor date
assignment = {
  repeatPattern: {
    interval: 2,
    unit: 'weeks',
    specificDays: ['monday'],
    anchorDate: Date.parse('2026-01-06') // First Monday
  }
}

// Check specific dates
isChoreActiveOnDate(assignment, new Date('2026-01-06')) // Week 0 → true
isChoreActiveOnDate(assignment, new Date('2026-01-13')) // Week 1 → false
isChoreActiveOnDate(assignment, new Date('2026-01-20')) // Week 2 → true
```

## Access Path

```
Main Screen (Child Selection)
  │
  ├─→ Select Child
  │
  ▼
Child Chore View
  │
  ├─→ Click "Calendar" Button
  │
  ▼
Calendar View
  │
  ├─→ Toggle [Week] or [Month]
  ├─→ Refresh ICS events (if configured)
  └─→ View chores and events
```

## Key Benefits

1. **Unified View**: See both app chores and external events together
2. **Planning Tool**: Parents can review child's schedule ahead of time
3. **Motivation**: Children can see what's coming up
4. **Flexibility**: Choose detail level with week vs month view
5. **Visual Clarity**: Color coding makes status immediately obvious
6. **No Setup Required**: Works even without external calendar

## Implementation Highlights

### Minimal Changes
- Only 4 source files modified
- Reuses existing components and styling
- Leverages existing date-fns utilities
- No new dependencies added

### Backward Compatible
- Calendar button only shows if onCalendarClick provided
- Works with or without ICS URL
- Gracefully handles missing chore data
- Maintains existing ICS event functionality

### Performance
- Efficient date range generation
- Memoized computations for chore filtering
- Map-based lookups for O(1) chore access
- Only computes visible date range

---

This visual guide complements the code to help understand the calendar view feature structure and behavior.
