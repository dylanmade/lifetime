import { useState } from "react";
import { CalendarIcon, ChevronLeft, ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Calendar } from "@/components/ui/calendar";
import { addDays, formatDayLabel, isSameDay } from "@/lib/datetime";

type Props = {
  selectedDate: Date;
  // Receives the newly-picked day (prev/next/calendar). Caller normalizes
  // (e.g. startOfDay) and applies any side effects (scroll reset).
  onSelectDate: (date: Date) => void;
};

// The `‹ date ›` day navigator shared by Summary and Timeline. Future days are
// disabled. Calendar open-state is internal.
export function DayNavHeader({ selectedDate, onSelectDate }: Props) {
  const [calendarOpen, setCalendarOpen] = useState(false);
  const isToday = isSameDay(selectedDate, new Date());

  return (
    <div className="flex flex-wrap items-center justify-between gap-4">
      <h1 className="text-2xl font-semibold tracking-tight">
        {formatDayLabel(selectedDate)}
      </h1>
      <div className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => onSelectDate(addDays(selectedDate, -1))}
          aria-label="Previous day"
        >
          <ChevronLeft />
        </Button>
        <Popover open={calendarOpen} onOpenChange={setCalendarOpen}>
          <PopoverTrigger asChild>
            <Button variant="outline" className="font-normal">
              <CalendarIcon />
              {selectedDate.toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
                year: "numeric",
              })}
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-auto p-0" align="end">
            <Calendar
              mode="single"
              selected={selectedDate}
              onSelect={(date) => {
                if (date) {
                  onSelectDate(date);
                  setCalendarOpen(false);
                }
              }}
              disabled={(date) => date > new Date()}
            />
          </PopoverContent>
        </Popover>
        <Button
          variant="ghost"
          size="icon"
          onClick={() => !isToday && onSelectDate(addDays(selectedDate, 1))}
          disabled={isToday}
          aria-label="Next day"
        >
          <ChevronRight />
        </Button>
      </div>
    </div>
  );
}
