"use client";

import type React from "react";
import { createContext, useContext, useMemo, useState } from "react";
import type { WaitingStatusData } from "@/lib/types";

type DataStreamContextValue = {
  waitingStatus: WaitingStatusData | undefined;
  setWaitingStatus: React.Dispatch<
    React.SetStateAction<WaitingStatusData | undefined>
  >;
};

const DataStreamContext = createContext<DataStreamContextValue | null>(null);

export function DataStreamProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [waitingStatus, setWaitingStatus] = useState<WaitingStatusData>();

  const value = useMemo(
    () => ({ setWaitingStatus, waitingStatus }),
    [waitingStatus]
  );

  return (
    <DataStreamContext.Provider value={value}>
      {children}
    </DataStreamContext.Provider>
  );
}

export function useDataStream() {
  const context = useContext(DataStreamContext);
  if (!context) {
    throw new Error("useDataStream must be used within a DataStreamProvider");
  }
  return context;
}
