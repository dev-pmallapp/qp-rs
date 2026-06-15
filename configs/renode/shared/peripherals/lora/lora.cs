//
// ESP32-C6 SPI2 (GPSPI2) LoRa Transceiver — Renode peripheral model
//
// Models a mock LoRa transceiver connected over SPI2:
//   - Writing to GPSPI_W0_REG through GPSPI_W15_REG (+0x098 to +0x0D4) accumulates the frame.
//   - Writing to GPSPI_CMD_REG (+0x00) with bit 24 (USR) set triggers the transfer.
//   - Reading GPSPI_CMD_REG returns 0 (USR bit cleared) to indicate idle / transaction complete.
//

using System;
using System.Text;
using System.Collections.Generic;
using Antmicro.Renode.Peripherals.Bus;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;

namespace Antmicro.Renode.Peripherals.SPI
{
    [AllowedTranslations(AllowedTranslation.ByteToDoubleWord | AllowedTranslation.WordToDoubleWord)]
    public class ESP32C6_LoraSpi : IDoubleWordPeripheral, IKnownSize
    {
        public ESP32C6_LoraSpi(IMachine machine)
        {
            this.machine = machine;
            wRegisters = new uint[16];
            Reset();
        }

        public long Size => 0x1000;

        public uint ReadDoubleWord(long offset)
        {
            if (offset >= W_START_OFFSET && offset <= W_END_OFFSET)
            {
                int index = (int)((offset - W_START_OFFSET) / 4);
                return wRegisters[index];
            }
            // All other reads (including CMD_REG +0x00) return 0 to signal transaction complete
            return 0u;
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            if (offset >= W_START_OFFSET && offset <= W_END_OFFSET)
            {
                int index = (int)((offset - W_START_OFFSET) / 4);
                wRegisters[index] = value;
            }
            else if (offset == CMD_OFFSET)
            {
                // Check if the USR command bit (bit 18) is set
                if ((value & (1u << 24)) != 0)
                {
                    // SPI transaction triggered! Extract bytes from the W registers
                    var dataBytes = new List<byte>();
                    foreach (var val in wRegisters)
                    {
                        dataBytes.Add((byte)(val & 0xFFu));
                        dataBytes.Add((byte)((val >> 8) & 0xFFu));
                        dataBytes.Add((byte)((val >> 16) & 0xFFu));
                        dataBytes.Add((byte)((val >> 24) & 0xFFu));
                    }

                    // Trim trailing zeros from the hex string
                    var hexBuilder = new StringBuilder();
                    foreach (var b in dataBytes)
                    {
                        hexBuilder.AppendFormat("{0:X2}", b);
                    }

                    string hexStr = hexBuilder.ToString().TrimEnd('0');
                    if (hexStr.Length % 2 != 0)
                    {
                        hexStr += "0"; // Keep even length for valid hex
                    }

                    if (!string.IsNullOrEmpty(hexStr))
                    {
                        this.Log(LogLevel.Info, "LoRa SPI Transceiver: Sent Telemetry Frame: {0}", hexStr);
                    }
                }
            }
        }

        public void Reset()
        {
            for (int i = 0; i < wRegisters.Length; i++)
            {
                wRegisters[i] = 0u;
            }
        }

        private readonly IMachine machine;
        private readonly uint[] wRegisters;

        private const long CMD_OFFSET = 0x000;
        private const long W_START_OFFSET = 0x098;
        private const long W_END_OFFSET = 0x0D4;
    }
}
