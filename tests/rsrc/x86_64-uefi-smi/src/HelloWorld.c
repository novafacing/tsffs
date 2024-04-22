// Copyright (C) 2024 Intel Corporation
// SPDX-License-Identifier: Apache-2.0

/** @file
  This sample application bases on HelloWorld PCD setting
  to print "UEFI Hello World!" to the UEFI Console.
  Copyright (c) 2006 - 2018, Intel Corporation. All rights reserved.<BR>
  SPDX-License-Identifier: BSD-2-Clause-Patent
**/

#include <Library/BaseMemoryLib.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/PcdLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiLib.h>
#include <Uefi.h>

#include <Guid/SmmLockBox.h>
#include <Library/LockBoxLib.h>

#include "tsffs.h"

/**
  The user Entry Point for Application. The user code starts with this function
  as the real entry point for the application.
  @param[in] ImageHandle    The firmware allocated handle for the EFI image.
  @param[in] SystemTable    A pointer to the EFI System Table.
  @retval EFI_SUCCESS       The entry point is executed successfully.
  @retval other             Some error occurs when executing this entry point.
**/
EFI_STATUS
EFIAPI
HelloWorldDxeInitialize(IN EFI_HANDLE ImageHandle, IN EFI_SYSTEM_TABLE *SystemTable) {
  Print(L"Initializing driver...");
  UINTN input_max_size = 64;
  UINTN input_size = input_max_size;
  UINT8 *input = (UINT8 *)AllocatePages(EFI_SIZE_TO_PAGES(input_max_size));

  if (!input) {
    return EFI_OUT_OF_RESOURCES;
  }

  SetMem((VOID *)input, input_max_size, 0x44);

  HARNESS_START(input, &input_size);

  GUID lockbox_guid;
  CopyMem(&lockbox_guid, input, sizeof(lockbox_guid));

  Print(L"Saving for GUID %g with input length %d\n", &lockbox_guid, input_size);

  EFI_STATUS Status = SaveLockBox(&lockbox_guid, input, input_size);

  Print(L"Got status from save: %d\n", Status);


  HARNESS_STOP();

  if (input) {
    FreePages(input, EFI_SIZE_TO_PAGES(input_max_size));
  }

  Print(L"Done...");

  return EFI_SUCCESS;
}
