import { cn } from "@/lib/utils"

interface IOSSpinnerProps {
  size?: number
  className?: string
}

export default function IOSSpinner({ size = 20, className }: IOSSpinnerProps) {
  return (
    <>
      <div className={cn("relative inline-block", className)} style={{ width: size, height: size }}>
        {[...Array(12)].map((_, i) => (
          <div
            key={i}
            className="absolute bg-current rounded-full"
            style={{
              width: size * 0.08,
              height: size * 0.25,
              left: "50%",
              top: "50%",
              transformOrigin: `0 ${size * 0.5}px`,
              transform: `translate(-50%, -100%) rotate(${i * 30}deg)`,
              opacity: 1 - i * 0.08,
              animation: "ios-spin 1s linear infinite",
              animationDelay: `${i * -0.083}s`,
            }}
          />
        ))}
      </div>
    </>
  )
}
