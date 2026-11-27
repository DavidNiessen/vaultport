// SPDX-License-Identifier: GPL-3.0-or-later
/** Opens a serial console: choose a port and line settings. */
import { useQuery } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Select } from '@/components/ui/controls';
import { Field, Input } from '@/components/ui/input';
import { Dialog } from '@/components/ui/overlay';
import { commands, errorMessage, newId } from '@/lib/ipc';
import { useApp } from '@/stores/app';

const BAUD = [300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

export function SerialDialog() {
  const { t } = useTranslation();
  const openTab = useApp((s) => s.openTab);
  const [open, setOpen] = useState(false);
  const [path, setPath] = useState('');
  const [baud, setBaud] = useState(115200);
  const [dataBits, setDataBits] = useState(8);
  const [stopBits, setStopBits] = useState(1);
  const [parity, setParity] = useState('none');
  const [flow, setFlow] = useState('none');
  const ports = useQuery({
    queryKey: ['serialPorts'],
    queryFn: () => commands.serialListPorts(),
    enabled: open,
  });

  useEffect(() => {
    const on = () => setOpen(true);
    window.addEventListener('vaultport:serial-picker', on);
    return () => window.removeEventListener('vaultport:serial-picker', on);
  }, []);

  const connect = () => {
    openTab({
      id: newId(),
      kind: 'terminal',
      title: path,
      hostId: null,
      protocol: 'serial',
      sessionId: null,
      status: 'connecting',
      unread: false,
      serial: { path, baudRate: baud, dataBits, stopBits, parity, flowControl: flow },
    });
    setOpen(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title={t('serial.title')}
      description={t('serial.description')}
      footer={
        <>
          <Button variant="ghost" onClick={() => setOpen(false)}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" disabled={!path} onClick={connect}>
            {t('hosts.connect')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Field
          label={t('serial.port')}
          error={ports.isError ? errorMessage(ports.error) : undefined}
        >
          {(id) => (
            <div className="flex gap-2">
              <Select id={id} value={path} onChange={(e) => setPath(e.target.value)}>
                <option value="">{t('serial.choose')}</option>
                {(ports.data ?? []).map((p) => (
                  <option key={p.path} value={p.path}>
                    {p.path}
                    {p.description ? ` — ${p.description}` : ''}
                  </option>
                ))}
              </Select>
              <Button
                size="icon"
                variant="ghost"
                aria-label={t('sftp.refresh')}
                onClick={() => void ports.refetch()}
              >
                <RefreshCw className="size-4" />
              </Button>
            </div>
          )}
        </Field>
        <Field label={t('serial.custom')}>
          {(id) => (
            <Input
              id={id}
              value={path}
              placeholder="/dev/ttyUSB0 / COM3"
              onChange={(e) => setPath(e.target.value)}
            />
          )}
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t('serial.baud')}>
            {(id) => (
              <Select id={id} value={baud} onChange={(e) => setBaud(Number(e.target.value))}>
                {BAUD.map((b) => (
                  <option key={b} value={b}>
                    {b}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t('serial.dataBits')}>
            {(id) => (
              <Select
                id={id}
                value={dataBits}
                onChange={(e) => setDataBits(Number(e.target.value))}
              >
                {[5, 6, 7, 8].map((b) => (
                  <option key={b} value={b}>
                    {b}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t('serial.parity')}>
            {(id) => (
              <Select id={id} value={parity} onChange={(e) => setParity(e.target.value)}>
                <option value="none">{t('serial.none')}</option>
                <option value="odd">{t('serial.odd')}</option>
                <option value="even">{t('serial.even')}</option>
              </Select>
            )}
          </Field>
          <Field label={t('serial.stopBits')}>
            {(id) => (
              <Select
                id={id}
                value={stopBits}
                onChange={(e) => setStopBits(Number(e.target.value))}
              >
                <option value={1}>1</option>
                <option value={2}>2</option>
              </Select>
            )}
          </Field>
          <Field label={t('serial.flow')} className="col-span-2">
            {(id) => (
              <Select id={id} value={flow} onChange={(e) => setFlow(e.target.value)}>
                <option value="none">{t('serial.none')}</option>
                <option value="software">XON/XOFF</option>
                <option value="hardware">RTS/CTS</option>
              </Select>
            )}
          </Field>
        </div>
      </div>
    </Dialog>
  );
}
