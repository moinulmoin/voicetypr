import { useMemo } from "react";
import { cn } from "@/lib/utils";
import { TranscriptionHistory } from "@/types";

interface ActivityGraphProps {
  history: TranscriptionHistory[];
  weeks?: number;
}

export function ActivityGraph({ history, weeks = 12 }: ActivityGraphProps) {
  const activityData = useMemo(() => {
    const today = new Date();
    today.setHours(23, 59, 59, 999);
    
    // Calculate the grid: always show 'weeks' weeks x 7 days
    const weeksData: number[][] = [];
    
    // Start from 'weeks' weeks ago, on a Sunday
    const startDate = new Date(today);
    startDate.setDate(startDate.getDate() - (weeks * 7) + 1);
    // Adjust to Sunday
    const dayOfWeek = startDate.getDay();
    if (dayOfWeek !== 0) {
      startDate.setDate(startDate.getDate() - dayOfWeek);
    }
    startDate.setHours(0, 0, 0, 0);
    
    // Create a map for quick lookup of history data
    const historyMap = new Map<string, number>();
    history.forEach(item => {
      const date = new Date(item.timestamp);
      const dateKey = date.toISOString().split('T')[0];
      historyMap.set(dateKey, (historyMap.get(dateKey) || 0) + 1);
    });
    
    // Build the grid week by week
    for (let w = 0; w < weeks; w++) {
      const week: number[] = [];
      for (let d = 0; d < 7; d++) {
        const currentDate = new Date(startDate);
        currentDate.setDate(startDate.getDate() + (w * 7) + d);
        
        // Check if this date is in the future
        if (currentDate > today) {
          week.push(0); // Future dates shown as empty
        } else {
          const dateKey = currentDate.toISOString().split('T')[0];
          week.push(historyMap.get(dateKey) || 0);
        }
      }
      weeksData.push(week);
    }
    
    // Calculate max count for intensity levels
    const maxCount = Math.max(1, ...Array.from(historyMap.values()));
    
    return { weeksData, maxCount };
  }, [history, weeks]);
  
  const getIntensityClass = (count: number, maxCount: number) => {
    if (count === 0) return "bg-gray-200 dark:bg-gray-800 hover:bg-gray-300 dark:hover:bg-gray-700"; // Very visible gray!
    
    const intensity = maxCount > 0 ? count / maxCount : 0;
    if (intensity > 0.75) return "bg-primary hover:bg-primary/90";
    if (intensity > 0.5) return "bg-primary/75 hover:bg-primary/65";
    if (intensity > 0.25) return "bg-primary/50 hover:bg-primary/40";
    return "bg-primary/25 hover:bg-primary/20";
  };
  
  const monthLabels = useMemo(() => {
    const labels: { month: string; position: number }[] = [];
    const today = new Date();
    const startDate = new Date(today);
    startDate.setDate(startDate.getDate() - (weeks * 7) + 1);
    
    let currentMonth = -1;
    for (let week = 0; week < weeks; week++) {
      const weekDate = new Date(startDate);
      weekDate.setDate(weekDate.getDate() + (week * 7));
      const month = weekDate.getMonth();
      
      if (month !== currentMonth) {
        currentMonth = month;
        labels.push({
          month: weekDate.toLocaleDateString('en-US', { month: 'short' }),
          position: week
        });
      }
    }
    
    return labels;
  }, [weeks]);
  
  const dayLabels = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];
  
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-sm font-medium">Activity</h3>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>Less</span>
          <div className="flex gap-1">
            <div className="w-3 h-3 rounded-sm bg-gray-200 dark:bg-gray-800" />
            <div className="w-3 h-3 rounded-sm bg-primary/25" />
            <div className="w-3 h-3 rounded-sm bg-primary/50" />
            <div className="w-3 h-3 rounded-sm bg-primary/75" />
            <div className="w-3 h-3 rounded-sm bg-primary" />
          </div>
          <span>More</span>
        </div>
      </div>
      
      <div className="flex gap-2">
        {/* Day labels */}
        <div className="flex flex-col gap-[2px] pr-1">
          <div className="h-3" /> {/* Spacer for month labels */}
          {dayLabels.map((day, index) => (
            <div key={index} className="h-3 text-[10px] text-muted-foreground flex items-center">
              {index % 2 === 1 ? day : ''}
            </div>
          ))}
        </div>
        
        <div className="flex-1">
          {/* Month labels */}
          <div className="flex h-3 mb-1 relative">
            {monthLabels.map(({ month, position }) => (
              <div
                key={`${month}-${position}`}
                className="absolute text-[10px] text-muted-foreground"
                style={{ left: `${(position / weeks) * 100}%` }}
              >
                {month}
              </div>
            ))}
          </div>
          
          {/* Activity grid */}
          <div className="flex gap-[2px]">
            {activityData.weeksData.map((week, weekIndex) => (
              <div key={weekIndex} className="flex flex-col gap-[2px]">
                {week.map((count, dayIndex) => {
                  const date = new Date();
                  date.setDate(date.getDate() - ((weeks - weekIndex - 1) * 7) - (6 - dayIndex));
                  const dateStr = date.toLocaleDateString('en-US', { 
                    month: 'short', 
                    day: 'numeric',
                    year: 'numeric'
                  });
                  
                  return (
                    <div
                      key={dayIndex}
                      className={cn(
                        "w-3 h-3 rounded-sm transition-colors cursor-pointer",
                        getIntensityClass(count, activityData.maxCount)
                      )}
                      title={`${count} transcription${count !== 1 ? 's' : ''} on ${dateStr}`}
                    />
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      </div>
      
      <div className="text-xs text-muted-foreground">
        {history.length} total transcriptions in the last {weeks} weeks
      </div>
    </div>
  );
}