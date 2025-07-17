import { useEffect, useRef, useState } from "react";

interface AudioWaveAnimationProps {
  audioLevel: number; // 0.0 to 1.0
  className?: string;
}

export function AudioWaveAnimation({ audioLevel, className = "" }: AudioWaveAnimationProps) {
  // Individual smoothed levels for each bar
  const barLevels = useRef([0, 0, 0, 0, 0]);
  const [barHeights, setBarHeights] = useState([3, 4, 3, 4, 3]);
  const animationFrame = useRef<number>();
  
  useEffect(() => {
    const animate = () => {
      // Different smoothing factors for each bar create wave effect
      const smoothingFactors = [0.15, 0.2, 0.25, 0.2, 0.15];
      const sensitivities = [0.8, 1.2, 1.0, 1.1, 0.9];
      const minHeights = [3, 4, 3, 4, 3];
      const maxHeights = [20, 28, 24, 26, 22];
      
      // Update each bar level independently
      barLevels.current = barLevels.current.map((currentLevel, i) => {
        // Add phase offset for wave motion
        const phaseOffset = Math.sin(Date.now() * 0.001 + i * 0.5) * 0.1;
        const targetLevel = audioLevel * sensitivities[i] + phaseOffset;
        
        // Smooth transition
        return currentLevel + (targetLevel - currentLevel) * smoothingFactors[i];
      });
      
      // Calculate heights with some organic variation
      const heights = barLevels.current.map((level, i) => {
        // Apply gentler exponential curve for better responsiveness to speech
        const exponentialLevel = Math.pow(Math.max(0, level), 1.2);
        const baseHeight = minHeights[i] + exponentialLevel * (maxHeights[i] - minHeights[i]);
        
        // Add subtle random variation
        const variation = (Math.random() - 0.5) * 1.5;
        
        return Math.max(minHeights[i], Math.min(maxHeights[i], baseHeight + variation));
      });
      
      setBarHeights(heights);
      animationFrame.current = requestAnimationFrame(animate);
    };
    
    animate();
    
    return () => {
      if (animationFrame.current) {
        cancelAnimationFrame(animationFrame.current);
      }
    };
  }, [audioLevel]);

  return (
    <div className={`flex items-center gap-[3px] ${className}`}>
      {barHeights.map((height, i) => (
        <div
          key={i}
          className="w-[3px] bg-white rounded-full"
          style={{
            height: `${height}px`,
            opacity: 0.9,
            // Remove CSS transitions since we're using requestAnimationFrame
            transition: 'none',
          }}
        />
      ))}
    </div>
  );
}